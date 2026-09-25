//! The GPU compositor must produce the same picture as the CPU compositor
//! (within resampling tolerance) for every kind of scene. Skipped with a
//! message on machines without a usable GPU adapter.

#![allow(
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use std::sync::{Arc, OnceLock};

use clipforge_core::project::{MediaRef, RefKind, Transition, TransitionKind};
use clipforge_core::{
    Aspect, Clip, ClipSource, Command, Fit, MediaId, Motion, Project, Quarter, Resolution, Ticks,
};
use clipforge_render::source::MapProvider;
use clipforge_render::{
    Compositor, Frame, FrameRenderer, GpuCompositor, RenderQuality, SourceImage, SourceProvider,
};

fn gpu() -> Option<&'static GpuCompositor> {
    static GPU: OnceLock<Option<GpuCompositor>> = OnceLock::new();
    let g = GPU.get_or_init(GpuCompositor::new).as_ref();
    if g.is_none() {
        eprintln!("no GPU adapter; skipping GPU comparison");
    }
    g
}

/// Smooth gradient with a distinct tint per image, so resampling filters
/// agree closely and any geometry error shows up as a large difference.
fn gradient(w: u32, h: u32, tint: u8) -> SourceImage {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(&[
                (x * 255 / w.max(1)) as u8,
                (y * 255 / h.max(1)) as u8,
                tint,
                255,
            ]);
        }
    }
    SourceImage {
        width: w,
        height: h,
        rgba: Arc::new(rgba),
    }
}

fn media(id: MediaId, kind: RefKind) -> MediaRef {
    MediaRef {
        id,
        kind,
        path: "/m".into(),
        fingerprint_hash: 1,
        size: 1,
        pixel_size: None,
        duration: (kind == RefKind::Video).then(|| Ticks::from_seconds(10)),
        captured_at_ms: None,
        name: "m".into(),
    }
}

/// Two 4-second photos (4:3 and 3:4) with a one-second transition.
fn scene(kind: TransitionKind) -> (Project, MapProvider) {
    let (a, b) = (MediaId::new(), MediaId::new());
    let mut provider = MapProvider::default();
    provider.images.insert(a, gradient(800, 600, 40));
    provider.images.insert(b, gradient(600, 800, 200));
    let mut second = Clip::photo(b, Ticks::from_seconds(4));
    second.transition_in = Transition {
        kind,
        duration: Ticks::SECOND,
    };
    let mut p = Project::new();
    Command::InsertClips {
        entries: vec![(0, Clip::photo(a, Ticks::from_seconds(4))), (1, second)],
        media: vec![media(a, RefKind::Photo), media(b, RefKind::Photo)],
    }
    .apply(&mut p)
    .unwrap();
    (p, provider)
}

/// Mean absolute channel difference and share of pixels off by more than 24.
fn diff(a: &Frame, b: &Frame) -> (f64, f64) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let mut sum = 0u64;
    let mut bad = 0u64;
    for (pa, pb) in a
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0)
    {
        let d = (0..3)
            .map(|c| i32::from(pa[c]).abs_diff(i32::from(pb[c])))
            .max()
            .unwrap_or(0);
        sum += u64::from(d);
        if d > 24 {
            bad += 1;
        }
    }
    let n = (a.width * a.height) as f64;
    (sum as f64 / n, bad as f64 / n)
}

fn assert_same(label: &str, p: &Project, t: Ticks, q: RenderQuality, sources: &dyn SourceProvider) {
    let Some(g) = gpu() else { return };
    let cpu = Compositor::new().render(p, t, q, sources);
    let gpu = g.render(p, t, q, sources);
    let (mean, bad) = diff(&cpu, &gpu);
    assert!(
        mean < 3.0 && bad < 0.02,
        "{label}: mean diff {mean:.2}, {:.2} % pixels off",
        bad * 100.0
    );
}

#[test]
fn stills_fit_cover_and_rotation_match() {
    let (mut p, provider) = scene(TransitionKind::Cut);
    assert_same(
        "contain",
        &p,
        Ticks::SECOND,
        RenderQuality::Preview,
        &provider,
    );
    assert_same(
        "contain portrait",
        &p,
        Ticks::from_seconds(6),
        RenderQuality::Preview,
        &provider,
    );
    Command::SetFit {
        indices: vec![0, 1],
        fit: Fit::Cover,
    }
    .apply(&mut p)
    .unwrap();
    assert_same(
        "cover",
        &p,
        Ticks::SECOND,
        RenderQuality::Preview,
        &provider,
    );
    Command::SetRotate {
        indices: vec![0],
        rotate: Quarter::Cw90,
    }
    .apply(&mut p)
    .unwrap();
    assert_same(
        "cover rotated 90",
        &p,
        Ticks::SECOND,
        RenderQuality::Preview,
        &provider,
    );
    Command::SetRotate {
        indices: vec![0],
        rotate: Quarter::Cw270,
    }
    .apply(&mut p)
    .unwrap();
    assert_same(
        "cover rotated 270",
        &p,
        Ticks::SECOND,
        RenderQuality::Preview,
        &provider,
    );
    Command::SetRotate {
        indices: vec![0],
        rotate: Quarter::Cw180,
    }
    .apply(&mut p)
    .unwrap();
    Command::SetFit {
        indices: vec![0],
        fit: Fit::Contain,
    }
    .apply(&mut p)
    .unwrap();
    assert_same(
        "contain rotated 180",
        &p,
        Ticks::SECOND,
        RenderQuality::Preview,
        &provider,
    );
    p.settings.aspect = Aspect::Portrait9x16;
    assert_same(
        "portrait project",
        &p,
        Ticks::SECOND,
        RenderQuality::Preview,
        &provider,
    );
    assert_same(
        "full hd",
        &p,
        Ticks::SECOND,
        RenderQuality::Full(Resolution::FullHd),
        &provider,
    );
}

#[test]
fn ken_burns_matches_over_the_clip() {
    for motion in Motion::ALL {
        let (mut p, provider) = scene(TransitionKind::Cut);
        Command::SetMotion {
            indices: vec![0],
            motion,
        }
        .apply(&mut p)
        .unwrap();
        for ms in [0, 1_300, 2_700, 3_900] {
            assert_same(
                &format!("{motion:?} at {ms} ms"),
                &p,
                Ticks::from_millis(ms),
                RenderQuality::Preview,
                &provider,
            );
        }
    }
}

#[test]
fn every_transition_matches_through_the_overlap() {
    for kind in TransitionKind::ALL {
        let (p, provider) = scene(kind);
        // Overlap is 3.0 .. 4.0 s.
        for ms in [3_000, 3_250, 3_500, 3_750] {
            assert_same(
                &format!("{kind:?} at {ms} ms"),
                &p,
                Ticks::from_millis(ms),
                RenderQuality::Preview,
                &provider,
            );
        }
    }
}

#[test]
fn openings_placeholders_and_the_end_match() {
    for kind in [
        TransitionKind::FadeThroughWhite,
        TransitionKind::SlideLeft,
        TransitionKind::WipeUp,
        TransitionKind::Zoom,
    ] {
        let (mut p, provider) = scene(TransitionKind::Cut);
        Command::SetTransition {
            indices: vec![0],
            transition: Transition {
                kind,
                duration: Ticks::SECOND,
            },
        }
        .apply(&mut p)
        .unwrap();
        for ms in [0, 400, 800] {
            assert_same(
                &format!("opening {kind:?} at {ms} ms"),
                &p,
                Ticks::from_millis(ms),
                RenderQuality::Preview,
                &provider,
            );
        }
    }
    let (p, _) = scene(TransitionKind::CrossDissolve);
    let empty = MapProvider::default();
    assert_same(
        "placeholders in a dissolve",
        &p,
        Ticks::from_millis(3_500),
        RenderQuality::Preview,
        &empty,
    );
    assert_same(
        "beyond the end",
        &p,
        Ticks::from_seconds(60),
        RenderQuality::Preview,
        &empty,
    );
    assert_same(
        "empty project",
        &Project::new(),
        Ticks::ZERO,
        RenderQuality::Preview,
        &empty,
    );
}

/// Video frames change every call; the GPU must re-upload each new frame.
struct Ramp {
    id: MediaId,
}

impl SourceProvider for Ramp {
    fn still(&self, _: MediaId, _: u32) -> Option<SourceImage> {
        None
    }
    fn video_frame(&self, media: MediaId, t: Ticks, _: u32) -> Option<SourceImage> {
        (media == self.id).then(|| gradient(320, 180, (t.as_seconds_f64() * 40.0) as u8))
    }
}

#[test]
fn video_frames_are_uploaded_per_frame() {
    let id = MediaId::new();
    let mut p = Project::new();
    let mut clip = Clip::video(id, Ticks::from_seconds(10));
    clip.source = ClipSource::Video {
        in_point: Ticks::SECOND,
        out_point: Ticks::from_seconds(5),
    };
    Command::InsertClips {
        entries: vec![(0, clip)],
        media: vec![media(id, RefKind::Video)],
    }
    .apply(&mut p)
    .unwrap();
    let provider = Ramp { id };
    for s in [0, 1, 2, 3] {
        assert_same(
            &format!("video at {s} s"),
            &p,
            Ticks::from_seconds(s),
            RenderQuality::Preview,
            &provider,
        );
    }
}

/// Writes a frame as a binary PPM (for looking at captions by hand).
fn dump(name: &str, f: &Frame) {
    let Ok(dir) = std::env::var("CLIPFORGE_DUMP") else {
        return;
    };
    let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
    for px in f.rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&px[..3]);
    }
    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.ppm")), out);
}

#[test]
fn text_track_matches_through_its_animations() {
    use clipforge_core::project::TitleBackground;
    use clipforge_core::{Font, TextAlign, TextItem, TextMotion};
    let (mut p, provider) = scene(TransitionKind::CrossDissolve);
    Command::InsertClips {
        entries: vec![(
            0,
            Clip::title(TitleBackground::Blue, Ticks::from_seconds(3)),
        )],
        media: vec![],
    }
    .apply(&mut p)
    .unwrap();
    // A title over the colour card, a boxed caption over the photos, and a
    // corner label; each with a different entrance.
    let mut title = TextItem::new(
        "Summer in Italy\nJuly 2024",
        Ticks::ZERO,
        Ticks::from_seconds(3),
    );
    title.style.font = Font::PlayfairDisplay;
    title.style.size = 1_000;
    title.style.bold = true;
    title.enter.kind = TextMotion::Zoom;
    let mut caption = TextItem::new(
        "Rome, the Colosseum at dusk",
        Ticks::from_seconds(3),
        Ticks::from_seconds(5),
    );
    caption.y = 8_600;
    caption.style.background = Some([0, 0, 0, 150]);
    caption.style.font = Font::Montserrat;
    caption.enter.kind = TextMotion::SlideUp;
    caption.exit.kind = TextMotion::WipeRight;
    let mut label = TextItem::new("July 2024", Ticks::from_seconds(3), Ticks::from_seconds(8));
    label.x = 2_000;
    label.y = 1_200;
    label.width = 3_000;
    label.style.font = Font::Caveat;
    label.style.align = TextAlign::Left;
    label.style.italic = true;
    label.style.color = [255, 220, 120, 255];
    label.enter.kind = TextMotion::WipeLeft;
    Command::InsertTexts {
        entries: vec![(0, title), (1, caption), (2, label)],
    }
    .apply(&mut p)
    .unwrap();
    for ms in [150, 1_500, 3_200, 4_000, 6_500, 7_800, 10_500] {
        for q in [
            RenderQuality::Preview,
            RenderQuality::Full(Resolution::FullHd),
        ] {
            assert_same(
                &format!("texts at {ms} ms"),
                &p,
                Ticks::from_millis(ms),
                q,
                &provider,
            );
        }
        dump(
            &format!("texts_{ms}"),
            &Compositor::new().render(
                &p,
                Ticks::from_millis(ms),
                RenderQuality::Preview,
                &provider,
            ),
        );
        if let Some(g) = gpu() {
            dump(
                &format!("texts_{ms}_gpu"),
                &g.render(
                    &p,
                    Ticks::from_millis(ms),
                    RenderQuality::Preview,
                    &provider,
                ),
            );
        }
    }
    // A text past the last clip still shows (over black).
    let tail = TextItem::new("The end", Ticks::from_seconds(10), Ticks::from_seconds(3));
    Command::InsertTexts {
        entries: vec![(3, tail)],
    }
    .apply(&mut p)
    .unwrap();
    assert_same(
        "text past the clips",
        &p,
        Ticks::from_millis(12_000),
        RenderQuality::Preview,
        &provider,
    );
}
