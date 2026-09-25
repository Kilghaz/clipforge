//! The CPU compositor.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use clipforge_core::project::{Clip, ClipSource, Motion, Quarter};
use clipforge_core::timeline::opening_overlap;
use clipforge_core::{Fit, MediaId, Project, Ticks};

use crate::draw::{Filter, RectF, draw};
use crate::frame::Frame;
use crate::layout::place;
use crate::quality::RenderQuality;
use crate::source::{SourceImage, SourceProvider};
use crate::text::{TextRenderer, draw_text, text_draw};
use crate::transition;

use crate::PLACEHOLDER_RGB;
/// Scaled-picture cache entries kept per compositor.
const CACHE_ENTRIES: usize = 48;

#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    media: MediaId,
    /// Source frame index for video, `None` for stills.
    time_key: Option<i64>,
    src_size: (u32, u32),
    frame: (u32, u32),
    fit: Fit,
    rotate: Quarter,
}

/// Renders frames of a project. Keeps a small cache of scaled pictures so
/// scrubbing over a still and exporting a still's many identical frames do
/// not rescale every time. Moving pictures (Ken Burns, video) bypass the
/// cache for stills with motion.
#[derive(Default)]
pub struct Compositor {
    cache: Mutex<HashMap<CacheKey, Arc<Frame>>>,
    order: Mutex<Vec<CacheKey>>,
    text: TextRenderer,
}

impl std::fmt::Debug for Compositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compositor").finish()
    }
}

impl crate::FrameRenderer for Compositor {
    fn render(
        &self,
        project: &Project,
        t: Ticks,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame {
        Compositor::render(self, project, t, quality, sources)
    }

    fn name(&self) -> &str {
        "cpu"
    }
}

impl Compositor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders the frame at timeline time `t`: clips, then the text track
    /// on top. Beyond the clips (or for an empty project) the picture is
    /// black.
    #[must_use]
    pub fn render(
        &self,
        project: &Project,
        t: Ticks,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame {
        let mut frame = self.render_clips(project, t, quality, sources);
        let size = (frame.width, frame.height);
        for item in project.texts.iter().filter(|i| i.visible_at(t)) {
            if let Some(img) = self.text.image(item, size)
                && let Some(d) = text_draw(item, &img, t, size)
            {
                draw_text(&mut frame, &img, &d);
            }
        }
        frame
    }

    fn render_clips(
        &self,
        project: &Project,
        t: Ticks,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame {
        let (w, h) = quality.frame_size(project.settings.aspect);
        let Some(at) = clipforge_core::timeline::frame_at(&project.clips, t) else {
            return Frame::black(w, h);
        };
        let filter = if quality.is_preview() {
            Filter::Fast
        } else {
            Filter::Sharp
        };
        let (cur_idx, cur_local) = at.current;
        let current = &project.clips[cur_idx];
        let frame = self.render_clip(current, cur_local, w, h, quality, sources);
        let kind = current.transition_in.kind;
        if let Some((out_idx, out_local)) = at.outgoing {
            let from = self.render_clip(&project.clips[out_idx], out_local, w, h, quality, sources);
            return transition::apply(kind, &from, &frame, at.progress, filter);
        }
        // The first clip plays its transition in from black (white for the white fade).
        let opening = if cur_idx == 0 {
            opening_overlap(&project.clips)
        } else {
            Ticks::ZERO
        };
        if opening > Ticks::ZERO && cur_local < opening {
            #[allow(clippy::cast_precision_loss)]
            let p = cur_local.flicks() as f32 / opening.flicks() as f32;
            let from = Frame::solid(w, h, transition::opening_colour(kind));
            // Coming from a solid colour, a fade is simply a fade-in; movement
            // transitions keep their movement.
            let open_kind = if kind.is_fade() {
                clipforge_core::TransitionKind::CrossDissolve
            } else {
                kind
            };
            return transition::apply(open_kind, &from, &frame, p, filter);
        }
        frame
    }

    /// Renders a single clip (no transitions) at `local` time within it.
    fn render_clip(
        &self,
        clip: &Clip,
        local: Ticks,
        w: u32,
        h: u32,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame {
        match clip.source {
            ClipSource::Title { background, .. } => {
                Frame::solid(w, h, crate::title_rgb(background))
            }
            _ => self.render_picture(clip, local, w, h, quality, sources),
        }
    }

    /// Renders a photo or video clip's picture.
    fn render_picture(
        &self,
        clip: &Clip,
        local: Ticks,
        w: u32,
        h: u32,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame {
        let want_edge = quality
            .source_edge(clipforge_core::Aspect::Landscape16x9)
            .max(w.max(h));
        let moving = clip.is_photo() && clip.motion != Motion::None;
        let (src, time_key) = match clip.source {
            ClipSource::Photo { .. } => {
                // A zoomed photo needs more source pixels to stay sharp.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let edge = if moving {
                    (f64::from(want_edge) * Motion::SCALE).ceil() as u32
                } else {
                    want_edge
                };
                (sources.still(clip.media, edge), None)
            }
            ClipSource::Video { in_point, .. } => {
                let t = in_point + local;
                // Quantise to milliseconds so identical requests hit the cache.
                let ms = t.flicks() / (clipforge_core::time::FLICKS_PER_SECOND / 1000);
                (sources.video_frame(clip.media, t, want_edge), Some(ms))
            }
            ClipSource::Title { .. } => (None, None),
        };
        let Some(src) = src else {
            return Frame::solid(w, h, PLACEHOLDER_RGB);
        };
        let filter = if quality.is_preview() {
            Filter::Fast
        } else {
            Filter::Sharp
        };
        let rotated = rotate(&src, clip.rotate);
        if moving {
            let d = clip.duration();
            #[allow(clippy::cast_precision_loss)]
            let progress = if d > Ticks::ZERO {
                local.flicks() as f64 / d.flicks() as f64
            } else {
                0.0
            };
            return compose(
                &rotated,
                w,
                h,
                clip.fit,
                clip.motion.camera(progress),
                filter,
            );
        }
        let key = CacheKey {
            media: clip.media,
            time_key,
            src_size: (src.width, src.height),
            frame: (w, h),
            fit: clip.fit,
            rotate: clip.rotate,
        };
        if let Some(hit) = self.cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
            return (*hit).clone();
        }
        let frame = compose(&rotated, w, h, clip.fit, (1.0, 0.0, 0.0), filter);
        self.remember(key, &frame);
        frame
    }

    fn remember(&self, key: CacheKey, frame: &Frame) {
        let (Ok(mut cache), Ok(mut order)) = (self.cache.lock(), self.order.lock()) else {
            return;
        };
        if cache.len() >= CACHE_ENTRIES && !order.is_empty() {
            let oldest = order.remove(0);
            cache.remove(&oldest);
        }
        order.push(key.clone());
        cache.insert(key, Arc::new(frame.clone()));
    }
}

/// Places `src` into a `w × h` black frame according to `fit` and a camera
/// `(zoom, x, y)` from [`Motion::camera`].
fn compose(
    src: &SourceImage,
    w: u32,
    h: u32,
    fit: Fit,
    camera: (f64, f64, f64),
    filter: Filter,
) -> Frame {
    let rect = place(src.width, src.height, w, h, fit);
    if rect.width == 0 || rect.height == 0 {
        return Frame::black(w, h);
    }
    let (zoom, cx, cy) = camera;
    // Fast path: the source already has the frame's size (video decoded at
    // preview size, or a still that matches) and nothing moves.
    if zoom == 1.0 && (src.width, src.height) == (w, h) && (rect.width, rect.height) == (w, h) {
        return Frame {
            width: w,
            height: h,
            rgba: src.rgba.as_ref().clone(),
        };
    }
    #[allow(clippy::cast_precision_loss)]
    let base = RectF {
        x: rect.x as f64,
        y: rect.y as f64,
        width: f64::from(rect.width),
        height: f64::from(rect.height),
    };
    // The camera window moves by (cx, cy) of the zoomed picture; the picture
    // moves the opposite way.
    let target = base.zoomed(
        zoom,
        -cx * zoom * base.width,
        -cy * zoom * base.height,
        w,
        h,
    );
    let mut frame = Frame::black(w, h);
    draw(&mut frame, src, target, filter);
    frame
}

/// Applies a user rotation on top of the already display-rotated source.
fn rotate(src: &SourceImage, q: Quarter) -> SourceImage {
    if q == Quarter::None {
        return src.clone();
    }
    let (w, h) = (src.width as usize, src.height as usize);
    let (nw, nh) = if q.swaps_dimensions() { (h, w) } else { (w, h) };
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match q {
                Quarter::Cw90 => (h - 1 - y, x),
                Quarter::Cw180 => (w - 1 - x, h - 1 - y),
                Quarter::Cw270 => (y, w - 1 - x),
                Quarter::None => (x, y),
            };
            let si = (y * w + x) * 4;
            let di = (ny * nw + nx) * 4;
            out[di..di + 4].copy_from_slice(&src.rgba[si..si + 4]);
        }
    }
    SourceImage {
        width: nw as u32,
        height: nh as u32,
        rgba: Arc::new(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MapProvider;
    use clipforge_core::project::{MediaRef, RefKind, Transition, TransitionKind};
    use clipforge_core::{Command, Resolution};

    fn media_ref(id: MediaId) -> MediaRef {
        MediaRef {
            id,
            kind: RefKind::Photo,
            path: "/p".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: None,
            duration: None,
            captured_at_ms: None,
            hdr: false,
            name: "p".into(),
        }
    }

    /// Two 3-second photos: red 4:3 and blue 16:9.
    fn scene() -> (Project, MapProvider, MediaId, MediaId) {
        let red = MediaId::new();
        let blue = MediaId::new();
        let mut provider = MapProvider::default();
        provider
            .images
            .insert(red, SourceImage::solid(400, 300, [255, 0, 0]));
        provider
            .images
            .insert(blue, SourceImage::solid(160, 90, [0, 0, 255]));
        let mut p = Project::new();
        Command::InsertClips {
            entries: vec![
                (0, Clip::photo(red, Ticks::from_seconds(3))),
                (1, Clip::photo(blue, Ticks::from_seconds(3))),
            ],
            media: vec![media_ref(red), media_ref(blue)],
        }
        .apply(&mut p)
        .unwrap();
        (p, provider, red, blue)
    }

    fn rgb(f: &Frame, x: u32, y: u32) -> [u8; 3] {
        let p = f.pixel(x, y);
        [p[0], p[1], p[2]]
    }

    #[test]
    fn contain_shows_bars_and_cover_fills() {
        let (mut p, provider, _, _) = scene();
        let c = Compositor::new();
        let f = c.render(&p, Ticks::SECOND, RenderQuality::Preview, &provider);
        assert_eq!((f.width, f.height), (960, 540));
        assert_eq!(rgb(&f, 480, 270), [255, 0, 0], "centre is the photo");
        assert_eq!(rgb(&f, 10, 270), [0, 0, 0], "pillarbox left");
        assert_eq!(rgb(&f, 949, 270), [0, 0, 0], "pillarbox right");
        assert_eq!(rgb(&f, 480, 2), [255, 0, 0], "full height");

        Command::SetFit {
            indices: vec![0],
            fit: Fit::Cover,
        }
        .apply(&mut p)
        .unwrap();
        let f = c.render(&p, Ticks::SECOND, RenderQuality::Preview, &provider);
        assert_eq!(rgb(&f, 10, 270), [255, 0, 0]);
        assert_eq!(rgb(&f, 949, 2), [255, 0, 0]);
    }

    #[test]
    fn same_aspect_source_fills_exactly_and_full_quality_sizes() {
        let (p, provider, _, _) = scene();
        let c = Compositor::new();
        let f = c.render(
            &p,
            Ticks::from_seconds(4),
            RenderQuality::Full(Resolution::FullHd),
            &provider,
        );
        assert_eq!((f.width, f.height), (1920, 1080));
        for (x, y) in [(0, 0), (1919, 1079), (960, 540)] {
            assert_eq!(rgb(&f, x, y), [0, 0, 255]);
        }
    }

    #[test]
    fn missing_source_gives_placeholder_and_end_is_black() {
        let (p, _, _, _) = scene();
        let c = Compositor::new();
        let f = c.render(
            &p,
            Ticks::SECOND,
            RenderQuality::Preview,
            &MapProvider::default(),
        );
        assert_eq!(rgb(&f, 480, 270), PLACEHOLDER_RGB);
        let f = c.render(
            &p,
            Ticks::from_seconds(60),
            RenderQuality::Preview,
            &MapProvider::default(),
        );
        assert_eq!(f, Frame::black(960, 540));
        let f = c.render(
            &Project::new(),
            Ticks::ZERO,
            RenderQuality::Preview,
            &MapProvider::default(),
        );
        assert_eq!(f, Frame::black(960, 540));
    }

    #[test]
    fn cross_dissolve_blends_midway() {
        let (mut p, provider, _, _) = scene();
        Command::SetTransition {
            indices: vec![1],
            transition: Transition {
                kind: TransitionKind::CrossDissolve,
                duration: Ticks::from_seconds(1),
            },
        }
        .apply(&mut p)
        .unwrap();
        let c = Compositor::new();
        // Overlap is 2.0..3.0; midpoint 2.5.
        let f = c.render(
            &p,
            Ticks::from_millis(2_500),
            RenderQuality::Preview,
            &provider,
        );
        let px = rgb(&f, 480, 270);
        assert!(
            (120..=136).contains(&px[0]) && px[1] == 0 && (120..=136).contains(&px[2]),
            "{px:?}"
        );
        let f = c.render(
            &p,
            Ticks::from_millis(2_000),
            RenderQuality::Preview,
            &provider,
        );
        assert_eq!(rgb(&f, 480, 270), [255, 0, 0]);
        let f = c.render(
            &p,
            Ticks::from_millis(3_000),
            RenderQuality::Preview,
            &provider,
        );
        assert_eq!(rgb(&f, 480, 270), [0, 0, 255]);
    }

    #[test]
    fn fade_through_black_is_black_in_the_middle_and_fades_in_at_start() {
        let (mut p, provider, _, _) = scene();
        let t = Transition {
            kind: TransitionKind::FadeThroughBlack,
            duration: Ticks::from_seconds(1),
        };
        Command::SetTransition {
            indices: vec![0, 1],
            transition: t,
        }
        .apply(&mut p)
        .unwrap();
        let c = Compositor::new();
        let f = c.render(
            &p,
            Ticks::from_millis(2_500),
            RenderQuality::Preview,
            &provider,
        );
        assert_eq!(rgb(&f, 480, 270), [0, 0, 0]);
        let f = c.render(
            &p,
            Ticks::from_millis(2_250),
            RenderQuality::Preview,
            &provider,
        );
        let px = rgb(&f, 480, 270);
        assert!(px[0] > 100 && px[0] < 156 && px[2] == 0, "{px:?}");
        // Start of the show fades in from black.
        let f = c.render(&p, Ticks::ZERO, RenderQuality::Preview, &provider);
        assert_eq!(rgb(&f, 480, 270), [0, 0, 0]);
        let f = c.render(
            &p,
            Ticks::from_millis(500),
            RenderQuality::Preview,
            &provider,
        );
        let px = rgb(&f, 480, 270);
        assert!(px[0] > 100 && px[0] < 156, "{px:?}");
    }

    #[test]
    fn first_clip_plays_its_transition_in_from_black_or_white() {
        let (mut p, provider, _, _) = scene();
        let t = Transition {
            kind: TransitionKind::FadeThroughWhite,
            duration: Ticks::from_seconds(1),
        };
        Command::SetTransition {
            indices: vec![0],
            transition: t,
        }
        .apply(&mut p)
        .unwrap();
        let c = Compositor::new();
        assert_eq!(
            rgb(
                &c.render(&p, Ticks::ZERO, RenderQuality::Preview, &provider),
                480,
                270
            ),
            [255, 255, 255]
        );
        let half = rgb(
            &c.render(
                &p,
                Ticks::from_millis(500),
                RenderQuality::Preview,
                &provider,
            ),
            480,
            270,
        );
        assert!(
            half[0] == 255 && (120..=136).contains(&half[1]),
            "half way from white to red: {half:?}"
        );
        // A slide pushes the first picture in over black from the right.
        let t = Transition {
            kind: TransitionKind::SlideLeft,
            duration: Ticks::from_seconds(1),
        };
        Command::SetTransition {
            indices: vec![0],
            transition: t,
        }
        .apply(&mut p)
        .unwrap();
        let f = c.render(
            &p,
            Ticks::from_millis(500),
            RenderQuality::Preview,
            &provider,
        );
        assert_eq!(rgb(&f, 100, 270), [0, 0, 0], "left still black");
        assert_eq!(
            rgb(&f, 700, 270),
            [255, 0, 0],
            "picture coming in on the right"
        );
        assert_eq!(
            rgb(
                &c.render(
                    &p,
                    Ticks::from_seconds(2),
                    RenderQuality::Preview,
                    &provider
                ),
                480,
                270
            ),
            [255, 0, 0]
        );
    }

    #[test]
    fn ken_burns_moves_the_camera_over_the_clip() {
        // Horizontal gradient source: red channel encodes the x position.
        let id = MediaId::new();
        let mut rgba = Vec::new();
        for _y in 0..90 {
            for x in 0..160u32 {
                #[allow(clippy::cast_possible_truncation)]
                rgba.extend_from_slice(&[(x * 255 / 159) as u8, 0, 0, 255]);
            }
        }
        let mut provider = MapProvider::default();
        provider.images.insert(
            id,
            SourceImage {
                width: 160,
                height: 90,
                rgba: Arc::new(rgba),
            },
        );
        let mut p = Project::new();
        Command::InsertClips {
            entries: vec![(0, Clip::photo(id, Ticks::from_seconds(4)))],
            media: vec![media_ref(id)],
        }
        .apply(&mut p)
        .unwrap();
        let c = Compositor::new();
        let left_edge = |p: &Project, t: Ticks| {
            rgb(&c.render(p, t, RenderQuality::Preview, &provider), 0, 270)[0]
        };
        let right_edge = |p: &Project, t: Ticks| {
            rgb(&c.render(p, t, RenderQuality::Preview, &provider), 959, 270)[0]
        };

        assert!(
            left_edge(&p, Ticks::ZERO) < 5,
            "no motion: whole picture visible"
        );
        Command::SetMotion {
            indices: vec![0],
            motion: Motion::ZoomIn,
        }
        .apply(&mut p)
        .unwrap();
        assert!(left_edge(&p, Ticks::ZERO) < 5, "zoom in starts wide");
        let late = left_edge(&p, Ticks::from_millis(3_999));
        assert!(
            (12..=30).contains(&late),
            "zoomed in by ~15 %: left edge shows x≈7 %: {late}"
        );

        Command::SetMotion {
            indices: vec![0],
            motion: Motion::PanLeft,
        }
        .apply(&mut p)
        .unwrap();
        let start = left_edge(&p, Ticks::ZERO);
        let end = left_edge(&p, Ticks::from_millis(3_999));
        assert!(start > end + 10, "camera travels left: {start} -> {end}");
        assert!(
            right_edge(&p, Ticks::from_millis(3_999)) < 250,
            "right part cropped at the end"
        );
        // Every moving frame is a fresh render (no stale cache hit).
        assert_ne!(
            left_edge(&p, Ticks::from_seconds(1)),
            left_edge(&p, Ticks::from_seconds(3))
        );
    }

    #[test]
    fn rotation_swaps_orientation() {
        let (mut p, provider, _, _) = scene();
        Command::SetRotate {
            indices: vec![0],
            rotate: Quarter::Cw90,
        }
        .apply(&mut p)
        .unwrap();
        let c = Compositor::new();
        let f = c.render(&p, Ticks::SECOND, RenderQuality::Preview, &provider);
        // 4:3 rotated becomes 3:4 portrait: taller than wide, so pillarboxed narrower.
        assert_eq!(rgb(&f, 480, 270), [255, 0, 0]);
        assert_eq!(rgb(&f, 200, 270), [0, 0, 0]);
        let src = SourceImage {
            width: 2,
            height: 1,
            rgba: Arc::new(vec![1, 1, 1, 255, 2, 2, 2, 255]),
        };
        let r = rotate(&src, Quarter::Cw90);
        assert_eq!((r.width, r.height), (1, 2));
        assert_eq!(&r.rgba[..4], &[1, 1, 1, 255]);
        assert_eq!(&r.rgba[4..], &[2, 2, 2, 255]);
    }

    struct TimedProvider {
        asked: std::sync::Mutex<Vec<Ticks>>,
    }

    impl SourceProvider for TimedProvider {
        fn still(&self, _: MediaId, _: u32) -> Option<SourceImage> {
            None
        }
        fn video_frame(&self, _: MediaId, t: Ticks, _: u32) -> Option<SourceImage> {
            self.asked.lock().unwrap().push(t);
            // Encode the second in the red channel so frames differ.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some(SourceImage::solid(
                16,
                9,
                [(t.as_seconds_f64() * 10.0) as u8, 0, 0],
            ))
        }
    }

    #[test]
    fn video_clips_ask_for_frames_at_trimmed_source_time() {
        let id = MediaId::new();
        let mut p = Project::new();
        let mut m = media_ref(id);
        m.kind = RefKind::Video;
        m.duration = Some(Ticks::from_seconds(10));
        let mut clip = Clip::video(id, Ticks::from_seconds(10));
        clip.source = clipforge_core::ClipSource::Video {
            in_point: Ticks::from_seconds(2),
            out_point: Ticks::from_seconds(5),
        };
        Command::InsertClips {
            entries: vec![(0, clip)],
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap();
        let provider = TimedProvider {
            asked: std::sync::Mutex::new(Vec::new()),
        };
        let c = Compositor::new();
        let f1 = c.render(&p, Ticks::SECOND, RenderQuality::Preview, &provider);
        let f2 = c.render(
            &p,
            Ticks::from_seconds(2),
            RenderQuality::Preview,
            &provider,
        );
        assert_eq!(
            provider.asked.lock().unwrap().as_slice(),
            [Ticks::from_seconds(3), Ticks::from_seconds(4)]
        );
        assert_ne!(
            f1, f2,
            "different source times are not served from the still cache"
        );
        assert_eq!(rgb(&f1, 480, 270), [30, 0, 0]);
    }

    #[test]
    fn cache_returns_identical_frames() {
        let (p, provider, _, _) = scene();
        let c = Compositor::new();
        let a = c.render(&p, Ticks::SECOND, RenderQuality::Preview, &provider);
        let b = c.render(
            &p,
            Ticks::from_seconds(2),
            RenderQuality::Preview,
            &provider,
        );
        assert_eq!(a, b);
        assert_eq!(c.cache.lock().unwrap().len(), 1);
        assert_eq!(place(0, 0, 10, 10, Fit::Contain).width, 0);
    }
}
