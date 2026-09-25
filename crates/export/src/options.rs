//! What the user chooses in the export dialog.

use clipforge_core::{FrameRate, Resolution};
use serde::{Deserialize, Serialize};

use crate::plan::Codec;

/// Quality tier, mapped to a bitrate ladder by the planner.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Good,
    #[default]
    Better,
    Best,
}

/// File format of the output.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    #[default]
    Mp4,
    /// QuickTime; same streams, preferred by Final Cut and some Mac tools.
    Mov,
}

impl Container {
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
        }
    }
}

/// Overrides under the dialog's "Advanced" disclosure. `None` means
/// "automatic": derived from the main options and the project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Advanced {
    /// Ignored for HDR, which is always HEVC.
    pub codec: Option<Codec>,
    /// Defaults to the project's frame rate.
    pub frame_rate: Option<FrameRate>,
    /// Replaces the quality ladder.
    pub video_bitrate_kbps: Option<u32>,
    pub audio_bitrate_kbps: Option<u32>,
    /// Ignored with "Optimise for YouTube", which is always MP4.
    pub container: Container,
}

/// Options as shown in the export dialog.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportOptions {
    pub resolution: Resolution,
    pub quality: Quality,
    /// Opt-in. Only meaningful if the timeline has HDR sources.
    pub hdr: bool,
    /// Applies YouTube's recommended container and codec settings.
    pub optimize_for_youtube: bool,
    #[serde(default)]
    pub advanced: Advanced,
}
