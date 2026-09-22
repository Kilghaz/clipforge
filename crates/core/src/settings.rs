//! Project-level settings shared by timeline, renderer and exporter.

use serde::{Deserialize, Serialize};

/// Output aspect ratio of a project.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    #[default]
    Landscape16x9,
    Portrait9x16,
}

impl Aspect {
    /// Width divided by height.
    #[must_use]
    pub fn ratio(self) -> f64 {
        match self {
            Aspect::Landscape16x9 => 16.0 / 9.0,
            Aspect::Portrait9x16 => 9.0 / 16.0,
        }
    }

    #[must_use]
    pub const fn is_portrait(self) -> bool {
        matches!(self, Aspect::Portrait9x16)
    }
}

/// Output resolution class as shown to the user.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// 1920×1080 (or 1080×1920 in portrait).
    #[default]
    FullHd,
    /// 3840×2160 (or 2160×3840 in portrait).
    Uhd4k,
}

impl Resolution {
    /// Pixel dimensions `(width, height)` for the given aspect.
    #[must_use]
    pub const fn dimensions(self, aspect: Aspect) -> (u32, u32) {
        let (long, short) = match self {
            Resolution::FullHd => (1920, 1080),
            Resolution::Uhd4k => (3840, 2160),
        };
        match aspect {
            Aspect::Landscape16x9 => (long, short),
            Aspect::Portrait9x16 => (short, long),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_follow_aspect() {
        assert_eq!(
            Resolution::FullHd.dimensions(Aspect::Landscape16x9),
            (1920, 1080)
        );
        assert_eq!(
            Resolution::FullHd.dimensions(Aspect::Portrait9x16),
            (1080, 1920)
        );
        assert_eq!(
            Resolution::Uhd4k.dimensions(Aspect::Landscape16x9),
            (3840, 2160)
        );
        assert_eq!(
            Resolution::Uhd4k.dimensions(Aspect::Portrait9x16),
            (2160, 3840)
        );
    }

    #[test]
    fn dimensions_are_even_and_match_ratio() {
        for res in [Resolution::FullHd, Resolution::Uhd4k] {
            for aspect in [Aspect::Landscape16x9, Aspect::Portrait9x16] {
                let (w, h) = res.dimensions(aspect);
                assert_eq!(w % 2, 0);
                assert_eq!(h % 2, 0);
                assert!((f64::from(w) / f64::from(h) - aspect.ratio()).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn settings_serialize_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&Aspect::Portrait9x16).unwrap(),
            "\"portrait9x16\""
        );
        assert_eq!(
            serde_json::to_string(&Resolution::Uhd4k).unwrap(),
            "\"uhd4k\""
        );
    }
}
