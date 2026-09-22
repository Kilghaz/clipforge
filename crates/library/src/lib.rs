//! The local media library.
//!
//! Files stay where they are. The library stores metadata, a content
//! [`Fingerprint`] for relinking, and paths to cached thumbnails and proxies.
//! The SQLite catalogue and the import pipeline arrive in Milestone 1.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod fingerprint;

pub use fingerprint::Fingerprint;
