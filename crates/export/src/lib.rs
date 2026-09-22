//! Export.
//!
//! The planner turns user-facing [`ExportOptions`] into an [`EncodePlan`]
//! (pure, snapshot-testable). The sidecar driver that feeds ffmpeg arrives in
//! Milestone 2.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod options;
pub mod plan;

pub use options::{ExportOptions, Quality};
pub use plan::{Codec, EncodePlan, PixelFormat};
