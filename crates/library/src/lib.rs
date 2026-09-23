//! The local media library.
//!
//! Files stay where they are. The library stores metadata, a content
//! [`Fingerprint`] for relinking, and paths to cached thumbnails and proxies
//! in a SQLite [`Catalogue`].

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod catalogue;
pub mod error;
pub mod fingerprint;
pub mod query;
pub mod record;
mod schema;

pub use catalogue::{Added, Catalogue};
pub use error::LibraryError;
pub use fingerprint::Fingerprint;
pub use query::{Query, Sort};
pub use record::{CloudState, MediaRecord, NewMedia, ProbeState, ThumbLevel, ThumbRecord};
