//! Classification of media files and their colour characteristics.

use serde::{Deserialize, Serialize};

/// Coarse media category, decided from the file extension before probing.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    Video,
    Audio,
    Unknown,
}

const PHOTO_EXT: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "webp", "heic", "heif", "hif", "avif", "tif", "tiff", "gif",
    "bmp", "dng", "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef",
];
const VIDEO_EXT: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "mts", "m2ts", "3gp",
];
const AUDIO_EXT: &[&str] = &[
    "mp3", "aac", "m4a", "wav", "flac", "ogg", "opus", "aif", "aiff",
];

impl MediaKind {
    /// Guesses the kind from a file extension (with or without leading dot,
    /// any case). Unknown extensions map to [`MediaKind::Unknown`].
    #[must_use]
    pub fn from_extension(ext: &str) -> MediaKind {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        if PHOTO_EXT.contains(&ext.as_str()) {
            MediaKind::Photo
        } else if VIDEO_EXT.contains(&ext.as_str()) {
            MediaKind::Video
        } else if AUDIO_EXT.contains(&ext.as_str()) {
            MediaKind::Audio
        } else {
            MediaKind::Unknown
        }
    }

    /// Guesses the kind from a path-like string by looking at its extension.
    #[must_use]
    pub fn from_path(path: &std::path::Path) -> MediaKind {
        path.extension()
            .and_then(|e| e.to_str())
            .map_or(MediaKind::Unknown, MediaKind::from_extension)
    }

    /// True for the camera RAW formats we open via their embedded preview.
    #[must_use]
    pub fn is_camera_raw(ext: &str) -> bool {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        matches!(
            ext.as_str(),
            "dng" | "cr2" | "cr3" | "nef" | "arw" | "orf" | "raf" | "rw2" | "pef"
        )
    }

    /// Extensions we accept when the user drops files, for filtering.
    pub fn supported_extensions() -> impl Iterator<Item = &'static str> {
        PHOTO_EXT.iter().chain(VIDEO_EXT).chain(AUDIO_EXT).copied()
    }
}

/// Transfer characteristic of a source, which decides whether it is HDR.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorTransfer {
    /// BT.709 / sRGB style gamma. Everything that is not HDR.
    #[default]
    Sdr,
    /// Hybrid log-gamma (ARIB STD-B67). iPhone video, broadcast HDR.
    Hlg,
    /// Perceptual quantiser (SMPTE ST 2084). HDR10, Dolby Vision profiles 5/8.1.
    Pq,
    /// SDR base image with an embedded gain map (Apple / ISO 21496-1 photos).
    GainMap,
}

impl ColorTransfer {
    #[must_use]
    pub const fn is_hdr(self) -> bool {
        !matches!(self, ColorTransfer::Sdr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn classifies_common_extensions_case_insensitively() {
        assert_eq!(MediaKind::from_extension("HEIC"), MediaKind::Photo);
        assert_eq!(MediaKind::from_extension(".jpg"), MediaKind::Photo);
        assert_eq!(MediaKind::from_extension("Mov"), MediaKind::Video);
        assert_eq!(MediaKind::from_extension("mkv"), MediaKind::Video);
        assert_eq!(MediaKind::from_extension("m4a"), MediaKind::Audio);
        assert_eq!(MediaKind::from_extension("txt"), MediaKind::Unknown);
        assert_eq!(MediaKind::from_extension(""), MediaKind::Unknown);
    }

    #[test]
    fn classifies_paths() {
        assert_eq!(
            MediaKind::from_path(Path::new("/x/IMG_0001.HEIC")),
            MediaKind::Photo
        );
        assert_eq!(
            MediaKind::from_path(Path::new("C:\\clips\\a.mp4")),
            MediaKind::Video
        );
        assert_eq!(MediaKind::from_path(Path::new("noext")), MediaKind::Unknown);
    }

    #[test]
    fn raw_is_photo_and_flagged() {
        assert_eq!(MediaKind::from_extension("dng"), MediaKind::Photo);
        assert!(MediaKind::is_camera_raw("ARW"));
        assert!(!MediaKind::is_camera_raw("jpg"));
    }

    #[test]
    fn supported_extensions_are_unique_lowercase() {
        let all: Vec<_> = MediaKind::supported_extensions().collect();
        let mut dedup = all.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(all.len(), dedup.len(), "duplicate extension in lists");
        assert!(all.iter().all(|e| {
            e.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        }));
    }

    #[test]
    fn hdr_flag() {
        assert!(!ColorTransfer::Sdr.is_hdr());
        assert!(ColorTransfer::Hlg.is_hdr());
        assert!(ColorTransfer::Pq.is_hdr());
        assert!(ColorTransfer::GainMap.is_hdr());
        assert_eq!(ColorTransfer::default(), ColorTransfer::Sdr);
    }
}
