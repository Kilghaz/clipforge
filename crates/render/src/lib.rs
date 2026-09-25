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

pub mod compositor;
pub mod draw;
pub mod frame;
pub mod layout;
pub mod quality;
pub mod source;
pub mod transition;

pub use compositor::Compositor;
pub use frame::Frame;
pub use layout::{Rect, place};
pub use quality::{PREVIEW_LONG_EDGE, RenderQuality};
pub use source::{SourceImage, SourceProvider};
