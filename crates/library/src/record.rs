//! Rows of the catalogue as Rust values.

use std::path::PathBuf;

use clipforge_core::MediaId;
use clipforge_media::{MediaInfo, MediaKind};
use serde::{Deserialize, Serialize};

use crate::fingerprint::Fingerprint;

/// Whether the file's bytes are available locally.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudState {
    /// Bytes are on disk.
    Local,
    /// A cloud placeholder; reading it would trigger a download.
    Placeholder,
    /// Not checked yet.
    #[default]
    Unknown,
}

impl CloudState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CloudState::Local => "local",
            CloudState::Placeholder => "placeholder",
            CloudState::Unknown => "unknown",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "local" => CloudState::Local,
            "placeholder" => CloudState::Placeholder,
            "unknown" => CloudState::Unknown,
            _ => return None,
        })
    }
}

/// Progress of metadata extraction for an item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeState {
    #[default]
    Pending,
    Done,
    Failed(String),
}

/// Thumbnail sizes kept in the cache. The value is the longest edge in pixels.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ThumbLevel {
    /// Grid cell.
    Small,
    /// Inspector and large grid cells.
    Medium,
    /// Preview-resolution still.
    Preview,
}

impl ThumbLevel {
    pub const ALL: [ThumbLevel; 3] = [ThumbLevel::Small, ThumbLevel::Medium, ThumbLevel::Preview];

    #[must_use]
    pub const fn long_edge(self) -> u32 {
        match self {
            ThumbLevel::Small => 256,
            ThumbLevel::Medium => 640,
            ThumbLevel::Preview => 1280,
        }
    }

    pub(crate) const fn as_i64(self) -> i64 {
        self.long_edge() as i64
    }

    pub(crate) fn from_i64(v: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_i64() == v)
    }
}

/// A cached thumbnail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThumbRecord {
    pub media_id: MediaId,
    pub level: ThumbLevel,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub generated_at_ms: i64,
}

/// Everything needed to register a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewMedia {
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
    pub mtime_ms: i64,
    pub kind: MediaKind,
    pub cloud_state: CloudState,
}

/// A library item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaRecord {
    pub id: MediaId,
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
    pub mtime_ms: i64,
    pub kind: MediaKind,
    pub cloud_state: CloudState,
    pub probe: ProbeState,
    /// Present once probing succeeded.
    pub info: Option<MediaInfo>,
    pub added_at_ms: i64,
}

impl MediaRecord {
    /// File name for display.
    #[must_use]
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Best available timestamp: capture time, else file modification time.
    #[must_use]
    pub fn sort_time_ms(&self) -> i64 {
        self.info
            .as_ref()
            .and_then(|i| i.captured_at_ms)
            .unwrap_or(self.mtime_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_levels_are_ascending_and_round_trip() {
        let edges: Vec<_> = ThumbLevel::ALL.iter().map(|l| l.long_edge()).collect();
        assert!(edges.windows(2).all(|w| w[0] < w[1]));
        for l in ThumbLevel::ALL {
            assert_eq!(ThumbLevel::from_i64(l.as_i64()), Some(l));
        }
        assert_eq!(ThumbLevel::from_i64(7), None);
    }

    #[test]
    fn cloud_state_round_trips() {
        for s in [
            CloudState::Local,
            CloudState::Placeholder,
            CloudState::Unknown,
        ] {
            assert_eq!(CloudState::parse(s.as_str()), Some(s));
        }
        assert_eq!(CloudState::parse("nope"), None);
    }
}
