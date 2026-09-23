//! The local media library.
//!
//! Files stay where they are. The library stores metadata, a content
//! [`Fingerprint`] for relinking, and paths to cached thumbnails and proxies
//! in a SQLite [`Catalogue`].

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod cache;
pub mod catalogue;
pub mod error;
pub mod fingerprint;
pub mod import;
pub mod library;
pub mod query;
pub mod record;
mod schema;

pub use cache::ThumbCache;
pub use catalogue::{Added, Catalogue};
pub use error::LibraryError;
pub use fingerprint::Fingerprint;
pub use import::{Candidate, enumerate};
pub use library::{Library, LibraryEvent, now_ms};
pub use query::{Query, Sort};
pub use record::{CloudState, MediaRecord, NewMedia, ProbeState, ThumbLevel, ThumbRecord};
