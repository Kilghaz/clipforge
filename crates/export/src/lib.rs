//! Export.
//!
//! The planner turns user-facing [`ExportOptions`] into an [`EncodePlan`]
//! (pure). [`Exporter`] renders timeline frames, converts them to
//! `yuv420p` and streams them into a separate `ffmpeg` process that encodes
//! with the best available hardware encoder (ADR-0003).

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod args;
pub mod audio;
pub mod encoders;
pub mod exporter;
pub mod frames;
pub mod options;
pub mod plan;
pub mod progress;
pub mod sources;
pub mod yuv;

pub use audio::{AudioSourceFactory, AudioStream};
pub use encoders::{Encoder, EncoderCatalog};
pub use exporter::{ExportError, ExportReport, Exporter};
pub use frames::{FrameRef, FrameSource, TimelineFrames};
pub use options::{ExportOptions, Quality};
pub use plan::{Codec, EncodePlan, PixelFormat};
pub use progress::ProgressLine;
pub use sources::{FileSources, sdr_frame};
