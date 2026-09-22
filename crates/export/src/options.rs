//! What the user chooses in the export dialog.

use clipforge_core::Resolution;
use serde::{Deserialize, Serialize};

/// Quality tier, mapped to a bitrate ladder by the planner.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Good,
    #[default]
    Better,
    Best,
}

/// Options as shown in the export dialog. Advanced overrides come later.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportOptions {
    pub resolution: Resolution,
    pub quality: Quality,
    /// Opt-in. Only meaningful if the timeline has HDR sources.
    pub hdr: bool,
    /// Applies YouTube's recommended container and codec settings.
    pub optimize_for_youtube: bool,
}
