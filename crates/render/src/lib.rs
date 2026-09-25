//! Frame rendering.
//!
//! `Compositor::render(project, time, quality, sources) -> Frame` is a pure
//! function of its inputs. Preview and export share the code and differ
//! only in [`RenderQuality`] and in the resolution of the sources handed in.
//!
//! This is the CPU compositor. It handles stills, fit modes, rotation and
//! the dissolve/fade transitions, which is everything a photo slideshow
//! needs; the wgpu compositor for motion effects arrives with Milestone 4
//! behind the same interface.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod colour;
pub mod compositor;
pub mod draw;
pub mod frame;
pub mod gpu;
pub mod layout;
pub mod quality;
pub mod source;
pub mod text;
pub mod transition;

pub use compositor::Compositor;
pub use gpu::GpuCompositor;

use clipforge_core::{Project, Ticks};

/// Colour drawn where a source is not available (yet).
pub const PLACEHOLDER_RGB: [u8; 3] = [40, 42, 48];

/// Colour of a colour card's background.
#[must_use]
pub const fn title_rgb(background: clipforge_core::project::TitleBackground) -> [u8; 3] {
    use clipforge_core::project::TitleBackground;
    match background {
        TitleBackground::Black => [0, 0, 0],
        TitleBackground::Charcoal => [38, 40, 46],
        TitleBackground::Blue => [22, 44, 84],
        TitleBackground::Red => [96, 24, 32],
        TitleBackground::White => [244, 242, 238],
    }
}

/// Anything that turns a project and a time into a frame. Implemented by
/// the CPU [`Compositor`] and the [`GpuCompositor`].
pub trait FrameRenderer: Send + Sync {
    /// Renders the frame at timeline time `t` (black beyond the end).
    fn render(
        &self,
        project: &Project,
        t: Ticks,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame;

    /// Renders the frame at `t` as HDR (HLG, BT.2020, 16-bit): HDR videos
    /// keep their range, SDR pictures sit at the reference white. `None`
    /// when this renderer has no HDR path (the CPU compositor).
    fn render_hlg(
        &self,
        project: &Project,
        t: Ticks,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Option<Frame16> {
        let _ = (project, t, quality, sources);
        None
    }

    /// Whether [`FrameRenderer::render_hlg`] produces frames (HDR export).
    fn supports_hlg(&self) -> bool {
        false
    }

    /// Short name for logs ("cpu", "gpu").
    fn name(&self) -> &str;
}

/// The fastest renderer available: the GPU compositor, or the CPU one when
/// no GPU adapter exists or `CLIPFORGE_RENDERER=cpu` is set.
#[must_use]
pub fn best_renderer() -> Box<dyn FrameRenderer> {
    let force_cpu =
        std::env::var("CLIPFORGE_RENDERER").is_ok_and(|v| v.eq_ignore_ascii_case("cpu"));
    if !force_cpu && let Some(gpu) = GpuCompositor::new() {
        return Box::new(gpu);
    }
    tracing::info!("using the CPU compositor");
    Box::new(Compositor::new())
}
pub use frame::{Frame, Frame16};
pub use layout::{Rect, place};
pub use quality::{PREVIEW_LONG_EDGE, RenderQuality};
pub use source::{HlgImage, SourceImage, SourceProvider, hlg_source, sdr_source, sdr_source_with};
