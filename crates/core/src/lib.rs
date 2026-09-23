//! ClipForge domain core.
//!
//! This crate holds the project model, the command/undo machinery and all
//! timeline maths. It must never depend on IO, threads, the OS or the UI.
//! Everything here is a pure function of its inputs so it can be tested
//! without any fixtures.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod ids;
pub mod settings;
pub mod time;

pub use ids::MediaId;
pub use settings::{Aspect, Resolution};
pub use time::{FrameRate, Ticks};

/// Version of the project file format written by this build.
pub const PROJECT_FORMAT_VERSION: u32 = 1;
