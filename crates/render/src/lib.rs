//! Frame rendering.
//!
//! `render(project, time, quality) -> Frame` is a pure function. Preview and
//! export share the code and differ only in [`RenderQuality`]. The wgpu
//! compositor arrives in Milestone 2.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use clipforge_core::{Aspect, Resolution};
use serde::{Deserialize, Serialize};

/// Longest edge of a preview frame in pixels.
pub const PREVIEW_LONG_EDGE: u32 = 960;

/// How much effort a render may spend.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderQuality {
    /// Reduced resolution, proxies allowed, may skip expensive effects.
    Preview,
    /// Full output resolution from original sources.
    Full(Resolution),
}

impl RenderQuality {
    /// Output frame size for this quality and aspect.
    #[must_use]
    pub const fn frame_size(self, aspect: Aspect) -> (u32, u32) {
        match self {
            RenderQuality::Full(res) => res.dimensions(aspect),
            RenderQuality::Preview => {
                let short = PREVIEW_LONG_EDGE * 9 / 16;
                match aspect {
                    Aspect::Landscape16x9 => (PREVIEW_LONG_EDGE, short),
                    Aspect::Portrait9x16 => (short, PREVIEW_LONG_EDGE),
                }
            }
        }
    }

    #[must_use]
    pub const fn is_preview(self) -> bool {
        matches!(self, RenderQuality::Preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_small_and_even() {
        let (w, h) = RenderQuality::Preview.frame_size(Aspect::Landscape16x9);
        assert_eq!((w, h), (960, 540));
        assert_eq!(
            RenderQuality::Preview.frame_size(Aspect::Portrait9x16),
            (540, 960)
        );
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
    }

    #[test]
    fn full_delegates_to_resolution() {
        assert_eq!(
            RenderQuality::Full(Resolution::Uhd4k).frame_size(Aspect::Landscape16x9),
            (3840, 2160)
        );
        assert!(!RenderQuality::Full(Resolution::FullHd).is_preview());
        assert!(RenderQuality::Preview.is_preview());
    }
}
