//! ClipForge domain core.
//!
//! This crate holds the project model, the command/undo machinery and all
//! timeline maths. It must never depend on IO, threads, the OS or the UI.
//! Everything here is a pure function of its inputs so it can be tested
//! without any fixtures.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod command;
pub mod history;
pub mod ids;
pub mod music;
pub mod persist;
pub mod project;
pub mod settings;
pub mod shuffle;
pub mod time;
pub mod timeline;

pub use command::{Command, CommandError, CommandLabel};
pub use history::History;
pub use ids::MediaId;
pub use music::{Music, Song, SongId};
pub use project::{
    Clip, ClipId, ClipSource, Fit, MediaRef, Motion, Project, ProjectSettings, Quarter, RefKind,
    Transition, TransitionKind,
};
pub use settings::{Aspect, Resolution};
pub use time::{FrameRate, Ticks};

/// Version of the project file format written by this build.
pub const PROJECT_FORMAT_VERSION: u32 = 1;
