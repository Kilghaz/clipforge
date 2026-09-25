//! Media vocabulary, probing and still-image decoding.
//!
//! Two backends exist today: the pure-Rust [`ImageBackend`] for the common
//! photo formats and [`FfmpegCli`], which drives the `ffprobe`/`ffmpeg`
//! executables for video, audio and everything the `image` crate cannot
//! read (HEIC, RAW previews). [`Backends`] picks per file. In-process
//! libav decoding for playback arrives with Milestone 3 (see ADR-0003).
//!
//! This crate will contain FFI later, so `unsafe` is denied but not
//! forbidden.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod backends;
pub mod error;
pub mod exif;
pub mod ffmpeg_cli;
pub mod image_backend;
pub mod info;
pub mod kind;
pub mod probe;
pub mod stream;

pub use backends::Backends;
pub use error::MediaError;
pub use ffmpeg_cli::{FfmpegCli, FfmpegLocation};
pub use image_backend::ImageBackend;
pub use info::{MediaInfo, Rotation};
pub use kind::{ColorTransfer, MediaKind};
pub use probe::{DecodedImage, Prober, StillDecoder};
pub use stream::{
    AUDIO_CHANNELS, AUDIO_SAMPLE_RATE, AudioReader, HdrImage, VideoFrame, VideoReader,
};

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;

    /// Path of a file in the workspace `fixtures/` directory.
    pub(crate) fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name)
    }
}
