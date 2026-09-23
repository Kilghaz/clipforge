//! The CPU compositor.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use clipforge_core::project::{Clip, Quarter, TransitionKind};
use clipforge_core::{Fit, MediaId, Project, Ticks};
use fast_image_resize as fr;

use crate::frame::Frame;
use crate::layout::place;
use crate::quality::RenderQuality;
use crate::source::{SourceImage, SourceProvider};

/// Colour drawn where a source is not available (yet).
const PLACEHOLDER_RGB: [u8; 3] = [40, 42, 48];
/// Scaled-picture cache entries kept per compositor.
const CACHE_ENTRIES: usize = 48;

#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    media: MediaId,
    src_size: (u32, u32),
    frame: (u32, u32),
    fit: Fit,
    rotate: Quarter,
}

/// Renders frames of a project. Cheap to clone-free share behind an `Arc`;
/// keeps a small cache of scaled pictures so scrubbing over a still and
/// exporting a still's many identical frames do not rescale every time.
#[derive(Default)]
pub struct Compositor {
    cache: Mutex<HashMap<CacheKey, Arc<Frame>>>,
    order: Mutex<Vec<CacheKey>>,
}

impl std::fmt::Debug for Compositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compositor").finish()
    }
}

impl Compositor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders the frame at timeline time `t`. Beyond the end (or for an
    /// empty project) the frame is black.
    #[must_use]
    pub fn render(
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
        let (cur_idx, _) = at.current;
        let current = &project.clips[cur_idx];
        let mut frame = self.render_clip(current, w, h, quality, sources);
        if let Some((out_idx, _)) = at.outgoing {
            let outgoing = &project.clips[out_idx];
            match current.transition_in.kind {
                TransitionKind::Cut => {}
                TransitionKind::CrossDissolve => {
                    let mut from = self.render_clip(outgoing, w, h, quality, sources);
                    from.blend_towards(&frame, at.progress);
                    frame = from;
                }
                TransitionKind::FadeThroughBlack => {
                    // First half: outgoing fades to black; second half: incoming fades in.
                    if at.progress < 0.5 {
                        let mut from = self.render_clip(outgoing, w, h, quality, sources);
                        from.darken(1.0 - at.progress * 2.0);
                        frame = from;
                    } else {
                        frame.darken((at.progress - 0.5) * 2.0);
                    }
                }
            }
        } else if cur_idx == 0 && current.transition_in.kind == TransitionKind::FadeThroughBlack {
            // Fade in from black at the very start.
            let overlap = current.transition_in.overlap();
            if overlap > Ticks::ZERO && at.current.1 < overlap {
                #[allow(clippy::cast_precision_loss)]
                let p = at.current.1.flicks() as f32 / overlap.flicks() as f32;
                frame.darken(p);
            }
        }
        frame
    }

    /// Renders a single clip's picture (no transitions).
    fn render_clip(
        &self,
        clip: &Clip,
        w: u32,
        h: u32,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame {
        let want_edge = quality
            .source_edge(clipforge_core::Aspect::Landscape16x9)
            .max(w.max(h));
        let Some(src) = sources.still(clip.media, want_edge) else {
            return Frame::solid(w, h, PLACEHOLDER_RGB);
        };
        let key = CacheKey {
            media: clip.media,
            src_size: (src.width, src.height),
            frame: (w, h),
            fit: clip.fit,
            rotate: clip.rotate,
        };
        if let Some(hit) = self.cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
            return (*hit).clone();
        }
        let rotated = rotate(&src, clip.rotate);
        let frame = compose(&rotated, w, h, clip.fit);
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

/// Scales `src` into a `w × h` black frame according to `fit`.
fn compose(src: &SourceImage, w: u32, h: u32, fit: Fit) -> Frame {
    let mut frame = Frame::black(w, h);
    let rect = place(src.width, src.height, w, h, fit);
    if rect.width == 0 || rect.height == 0 {
        return frame;
    }
    let scaled = match fit {
        Fit::Contain => resize(src, rect.width, rect.height, None),
        Fit::Cover => {
            // Crop the source to the visible region first, then resize to the frame.
            let sx = f64::from(src.width) / f64::from(rect.width);
            let sy = f64::from(src.height) / f64::from(rect.height);
            let crop_w = f64::from(w) * sx;
            let crop_h = f64::from(h) * sy;
            let left = (f64::from(src.width) - crop_w) / 2.0;
            let top = (f64::from(src.height) - crop_h) / 2.0;
            resize(
                src,
                w,
                h,
                Some((
                    left.max(0.0),
                    top.max(0.0),
                    crop_w.min(f64::from(src.width)),
                    crop_h.min(f64::from(src.height)),
                )),
            )
        }
    };
    match fit {
        Fit::Contain => frame.blit(&scaled, rect.x, rect.y),
        Fit::Cover => frame.blit(&scaled, 0, 0),
    }
    frame
}

fn resize(src: &SourceImage, w: u32, h: u32, crop: Option<(f64, f64, f64, f64)>) -> Frame {
    let Some(src_img) =
        fr::images::ImageRef::new(src.width, src.height, &src.rgba, fr::PixelType::U8x4).ok()
    else {
        return Frame::solid(w, h, PLACEHOLDER_RGB);
    };
    let mut dst = fr::images::Image::new(w, h, fr::PixelType::U8x4);
    let mut options =
        fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3));
    if let Some((l, t, cw, ch)) = crop {
        options = options.crop(l, t, cw, ch);
    }
    let mut resizer = fr::Resizer::new();
    if resizer.resize(&src_img, &mut dst, &options).is_err() {
        return Frame::solid(w, h, PLACEHOLDER_RGB);
    }
    let mut rgba = dst.into_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    Frame {
        width: w,
        height: h,
        rgba,
    }
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
    use clipforge_core::project::{MediaRef, RefKind, Transition};
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
