//! What probing a file tells us.

use clipforge_core::{FrameRate, Ticks};
use serde::{Deserialize, Serialize};

use crate::kind::{ColorTransfer, MediaKind};

/// Rotation stored in metadata that a viewer must apply (EXIF orientation,
/// QuickTime display matrix). Clockwise degrees.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    #[default]
    None,
    Cw90,
    Cw180,
    Cw270,
}

impl Rotation {
    /// Maps an EXIF orientation tag (1–8) to a rotation, ignoring mirroring.
    #[must_use]
    pub const fn from_exif(orientation: u32) -> Rotation {
        match orientation {
            3 | 4 => Rotation::Cw180,
            5 | 6 => Rotation::Cw90,
            7 | 8 => Rotation::Cw270,
            _ => Rotation::None,
        }
    }

    /// Maps a degree value (as found in QuickTime `rotate` metadata) to a rotation.
    #[must_use]
    pub fn from_degrees(degrees: i32) -> Rotation {
        match degrees.rem_euclid(360) {
            90 => Rotation::Cw90,
            180 => Rotation::Cw180,
            270 => Rotation::Cw270,
            _ => Rotation::None,
        }
    }

    #[must_use]
    pub const fn degrees(self) -> u32 {
        match self {
            Rotation::None => 0,
            Rotation::Cw90 => 90,
            Rotation::Cw180 => 180,
            Rotation::Cw270 => 270,
        }
    }

    /// True if width and height swap when displayed.
    #[must_use]
    pub const fn swaps_dimensions(self) -> bool {
        matches!(self, Rotation::Cw90 | Rotation::Cw270)
    }
}

/// Metadata of a successfully probed file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub kind: MediaKind,
    /// Stored (unrotated) pixel dimensions. `None` for audio.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub rotation: Rotation,
    /// `None` for still images.
    pub duration: Option<Ticks>,
    pub frame_rate: Option<FrameRate>,
    pub has_audio: bool,
    pub transfer: ColorTransfer,
    /// Capture time as Unix milliseconds (EXIF DateTimeOriginal, QuickTime
    /// creation_time). `None` if the file carries none.
    pub captured_at_ms: Option<i64>,
    /// Container or codec name for display, e.g. "jpeg", "hevc".
    pub codec: String,
}

impl MediaInfo {
    /// Dimensions as displayed, i.e. after applying `rotation`.
    #[must_use]
    pub fn display_size(&self) -> Option<(u32, u32)> {
        let (w, h) = (self.width?, self.height?);
        Some(if self.rotation.swaps_dimensions() {
            (h, w)
        } else {
            (w, h)
        })
    }

    #[must_use]
    pub const fn is_hdr(&self) -> bool {
        self.transfer.is_hdr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_orientation_maps_to_rotation() {
        assert_eq!(Rotation::from_exif(1), Rotation::None);
        assert_eq!(Rotation::from_exif(6), Rotation::Cw90);
        assert_eq!(Rotation::from_exif(3), Rotation::Cw180);
        assert_eq!(Rotation::from_exif(8), Rotation::Cw270);
        assert_eq!(Rotation::from_exif(0), Rotation::None);
        assert_eq!(Rotation::from_exif(99), Rotation::None);
    }

    #[test]
    fn degrees_normalise() {
        assert_eq!(Rotation::from_degrees(-90), Rotation::Cw270);
        assert_eq!(Rotation::from_degrees(450), Rotation::Cw90);
        assert_eq!(Rotation::from_degrees(45), Rotation::None);
        assert_eq!(Rotation::Cw270.degrees(), 270);
    }

    #[test]
    fn display_size_swaps_for_quarter_turns() {
        let info = MediaInfo {
            kind: MediaKind::Photo,
            width: Some(4000),
            height: Some(3000),
            rotation: Rotation::Cw90,
            duration: None,
            frame_rate: None,
            has_audio: false,
            transfer: ColorTransfer::Sdr,
            captured_at_ms: None,
            codec: "jpeg".into(),
        };
        assert_eq!(info.display_size(), Some((3000, 4000)));
        assert!(!info.is_hdr());
    }
}
