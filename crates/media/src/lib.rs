//! Media vocabulary and decoder abstractions.
//!
//! Backends (FFmpeg, Apple ImageIO, the `image` crate) arrive with Milestone
//! 1 and 3. This crate is the only one besides `platform` that will contain
//! FFI, so `unsafe` is denied but not forbidden.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod kind;

pub use kind::{ColorTransfer, MediaKind};
