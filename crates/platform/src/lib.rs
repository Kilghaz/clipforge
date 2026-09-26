//! Operating system glue.
//!
//! Everything that differs between macOS and Windows lives here behind small
//! traits so the rest of the workspace stays platform-neutral and testable.
//! Drag-and-drop payloads, OS thumbnails and cloud placeholder detection
//! arrive with Milestone 1. This crate will contain FFI, so `unsafe` is
//! denied but not forbidden.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod cloud;
pub mod codecs;
pub mod dirs;
pub mod reveal;

pub use cloud::{CloudStatus, IcloudStub, cloud_status, icloud_stub};
pub use codecs::{HEVC_STORE_URI, hevc_playback};
pub use dirs::AppDirs;
pub use reveal::{open_uri, reveal_in_file_manager};
