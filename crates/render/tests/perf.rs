//! Frame-time measurements for the preview budget (33 ms at 30 fps).
//! Run: `cargo test -p clipforge-render --test perf -- --ignored --nocapture`
//! (add `--release` for release numbers).

#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::cast_precision_loss)]

use std::sync::Arc;
use std::time::Instant;

use clipforge_core::project::{MediaRef, RefKind, Transition, TransitionKind};
use clipforge_core::{Clip, Command, MediaId, Motion, Project, Resolution, Ticks};
use clipforge_render::source::MapProvider;
use clipforge_render::{Compositor, RenderQuality, SourceImage};

/// Two 1280 × 853 photos (the preview thumbnail size), the second with a
/// dissolve, both with Ken Burns.
pub fn scene(edge: u32) -> (Project, MapProvider) {
    let mut provider = MapProvider::default();
    let mut media = Vec::new();
    let mut entries = Vec::new();
    for i in 0..2u32 {
        let id = MediaId::new();
        let (w, h) = (edge, edge * 2 / 3);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[
                    (x * 255 / w) as u8,
                    (y * 255 / h) as u8,
                    (i * 200) as u8,
                    255,
                ]);
            }
        }
        provider.images.insert(
            id,
            SourceImage {
                width: w,
                height: h,
                rgba: Arc::new(rgba),
            },
        );
        media.push(MediaRef {
            id,
            kind: RefKind::Photo,
            path: "/p".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: Some((w, h)),
            duration: None,
            captured_at_ms: None,
            name: "p".into(),
        });
        let mut c = Clip::photo(id, Ticks::from_seconds(4));
        c.motion = if i == 0 {
            Motion::ZoomIn
        } else {
            Motion::PanLeft
        };
        if i == 1 {
            c.transition_in = Transition {
                kind: TransitionKind::CrossDissolve,
                duration: Ticks::SECOND,
            };
        }
        entries.push((i as usize, c));
    }
    let mut p = Project::new();
    Command::InsertClips { entries, media }
        .apply(&mut p)
        .unwrap();
    (p, provider)
}

fn measure(label: &str, frames: u32, mut f: impl FnMut(u32)) {
    f(0); // warm-up (allocations, caches)
    let t0 = Instant::now();
    for i in 0..frames {
        f(i);
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
    eprintln!("{label:<44} {ms:7.2} ms/frame");
}

#[test]
#[ignore = "performance measurement, run manually"]
fn cpu_frame_times() {
    let c = Compositor::new();
    let (p, provider) = scene(1280);
    measure("cpu preview, ken burns", 60, |i| {
        let _ = c.render(
            &p,
            Ticks::from_millis(i64::from(i) * 33),
            RenderQuality::Preview,
            &provider,
        );
    });
    measure("cpu preview, dissolve + 2x ken burns", 30, |i| {
        let _ = c.render(
            &p,
            Ticks::from_millis(3_000 + i64::from(i) * 33),
            RenderQuality::Preview,
            &provider,
        );
    });
    let (p4, provider4) = scene(4032);
    measure("cpu 4K export, dissolve + 2x ken burns", 5, |i| {
        let _ = c.render(
            &p4,
            Ticks::from_millis(3_000 + i64::from(i) * 33),
            RenderQuality::Full(Resolution::Uhd4k),
            &provider4,
        );
    });
}
