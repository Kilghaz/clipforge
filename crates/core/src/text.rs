//! Text on its own track above the clips: titles, captions and labels that
//! sit anywhere in time and anywhere in the frame, independent of clips.
//!
//! Geometry is stored as integer fractions of the frame (1/10 000) so the
//! model stays `Eq`/`Hash` and a text looks the same at every resolution.
//! Items later in `Project::texts` are drawn on top.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::time::Ticks;

/// Fractions of the frame are stored in these units (10 000 = whole frame).
pub const FRAME_UNITS: i32 = 10_000;

/// Identity of a text item. Survives moves and undo.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextId(Uuid);

impl TextId {
    #[must_use]
    pub fn new() -> Self {
        TextId(Uuid::new_v4())
    }
}

impl Default for TextId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TextId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

/// The bundled fonts (all SIL Open Font License). Serialised names are
/// stable; new fonts are appended.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Font {
    /// Neutral sans serif.
    #[default]
    Inter,
    /// Geometric sans, popular for video titles.
    Montserrat,
    /// High-contrast serif.
    PlayfairDisplay,
    /// Tall condensed capitals.
    BebasNeue,
    /// Flowing script.
    DancingScript,
    /// Casual handwriting.
    Caveat,
}

impl Font {
    pub const ALL: [Font; 6] = [
        Font::Inter,
        Font::Montserrat,
        Font::PlayfairDisplay,
        Font::BebasNeue,
        Font::DancingScript,
        Font::Caveat,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    #[must_use]
    pub fn from_index(index: usize) -> Font {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

/// Horizontal alignment of the lines inside the text box.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap_or(1)
    }

    #[must_use]
    pub fn from_index(index: usize) -> TextAlign {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

/// How a text looks.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default)]
    pub font: Font,
    /// Font size in 1/10 000 of the frame's short side (600 = 6 %).
    #[serde(default = "default_size")]
    pub size: u16,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    /// Straight-alpha RGBA.
    #[serde(default = "white")]
    pub color: [u8; 4],
    #[serde(default)]
    pub align: TextAlign,
    /// A box behind the text (straight-alpha RGBA), or none.
    #[serde(default)]
    pub background: Option<[u8; 4]>,
    /// Soft drop shadow for legibility over pictures.
    #[serde(default = "yes")]
    pub shadow: bool,
}

fn default_size() -> u16 {
    TextStyle::DEFAULT_SIZE
}

fn white() -> [u8; 4] {
    [255, 255, 255, 255]
}

fn yes() -> bool {
    true
}

impl TextStyle {
    pub const DEFAULT_SIZE: u16 = 600;
    pub const MIN_SIZE: u16 = 150;
    pub const MAX_SIZE: u16 = 3000;
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            font: Font::Inter,
            size: Self::DEFAULT_SIZE,
            bold: false,
            italic: false,
            color: white(),
            align: TextAlign::Center,
            background: None,
            shadow: true,
        }
    }
}

/// How a text appears or disappears; the same movements as clip
/// transitions, applied to the text alone.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextMotion {
    /// Appears / disappears at once.
    Cut,
    #[default]
    Fade,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    Zoom,
}

impl TextMotion {
    pub const ALL: [TextMotion; 11] = [
        TextMotion::Cut,
        TextMotion::Fade,
        TextMotion::SlideLeft,
        TextMotion::SlideRight,
        TextMotion::SlideUp,
        TextMotion::SlideDown,
        TextMotion::WipeLeft,
        TextMotion::WipeRight,
        TextMotion::WipeUp,
        TextMotion::WipeDown,
        TextMotion::Zoom,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    #[must_use]
    pub fn from_index(index: usize) -> TextMotion {
        Self::ALL.get(index).copied().unwrap_or(TextMotion::Cut)
    }
}

/// An appearance or disappearance and how long it takes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextAnimation {
    pub kind: TextMotion,
    pub duration: Ticks,
}

impl Default for TextAnimation {
    fn default() -> Self {
        TextAnimation {
            kind: TextMotion::Fade,
            duration: Ticks::from_millis(500),
        }
    }
}

/// One text on the text track.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextItem {
    pub id: TextId,
    /// Line breaks are kept.
    pub text: String,
    /// Timeline position.
    pub start: Ticks,
    pub duration: Ticks,
    /// Centre of the text box, in 1/10 000 of the frame width / height.
    pub x: i32,
    pub y: i32,
    /// Width of the text box (lines wrap inside), 1/10 000 of frame width.
    pub width: i32,
    #[serde(default)]
    pub style: TextStyle,
    #[serde(default)]
    pub enter: TextAnimation,
    #[serde(default)]
    pub exit: TextAnimation,
}

impl TextItem {
    pub const MIN_DURATION: Ticks = Ticks::from_millis(200);
    pub const MIN_WIDTH: i32 = 500;

    /// A centred text with the default style.
    #[must_use]
    pub fn new(text: impl Into<String>, start: Ticks, duration: Ticks) -> TextItem {
        TextItem {
            id: TextId::new(),
            text: text.into(),
            start,
            duration,
            x: FRAME_UNITS / 2,
            y: FRAME_UNITS / 2,
            width: FRAME_UNITS * 8 / 10,
            style: TextStyle::default(),
            enter: TextAnimation::default(),
            exit: TextAnimation::default(),
        }
    }

    #[must_use]
    pub fn end(&self) -> Ticks {
        self.start + self.duration
    }

    /// Whether the item is on screen at `t`.
    #[must_use]
    pub fn visible_at(&self, t: Ticks) -> bool {
        t >= self.start && t < self.end()
    }

    /// Animation state at `t`: how far the entrance and the exit have come
    /// (`1` = fully shown). Each animation takes at most half the item.
    #[must_use]
    pub fn phase_at(&self, t: Ticks) -> TextPhase {
        let local = t - self.start;
        let half = self.duration / 2;
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let progress = |elapsed: Ticks, anim: TextAnimation| -> f32 {
            let d = anim.duration.min(half);
            if anim.kind == TextMotion::Cut || d <= Ticks::ZERO {
                1.0
            } else {
                (elapsed.flicks() as f64 / d.flicks() as f64).clamp(0.0, 1.0) as f32
            }
        };
        TextPhase {
            enter: (self.enter.kind, progress(local, self.enter)),
            exit: (self.exit.kind, progress(self.end() - t, self.exit)),
        }
    }

    /// Checks value ranges.
    pub fn validate(&self) -> Result<(), String> {
        if self.duration < Self::MIN_DURATION {
            return Err("text is too short".to_owned());
        }
        if self.start < Ticks::ZERO {
            return Err("text starts before the show".to_owned());
        }
        if self.width < Self::MIN_WIDTH || self.width > FRAME_UNITS * 2 {
            return Err("text box width out of range".to_owned());
        }
        if !(TextStyle::MIN_SIZE..=TextStyle::MAX_SIZE).contains(&self.style.size) {
            return Err("font size out of range".to_owned());
        }
        if self.enter.duration < Ticks::ZERO || self.exit.duration < Ticks::ZERO {
            return Err("negative animation".to_owned());
        }
        Ok(())
    }
}

/// Entrance and exit progress of a text at some instant (`1` = done / not
/// yet leaving).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextPhase {
    pub enter: (TextMotion, f32),
    pub exit: (TextMotion, f32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_is_visible_over_its_span_only() {
        let t = TextItem::new("Hi", Ticks::from_seconds(2), Ticks::from_seconds(3));
        assert!(!t.visible_at(Ticks::from_millis(1_999)));
        assert!(t.visible_at(Ticks::from_seconds(2)));
        assert!(t.visible_at(Ticks::from_millis(4_999)));
        assert!(!t.visible_at(Ticks::from_seconds(5)));
        assert_eq!(t.end(), Ticks::from_seconds(5));
    }

    #[test]
    fn animations_run_at_both_ends_and_are_capped_at_half() {
        let t = TextItem::new("Hi", Ticks::ZERO, Ticks::from_seconds(4));
        let p = t.phase_at(Ticks::from_millis(250));
        assert_eq!(p.enter.0, TextMotion::Fade);
        assert!((p.enter.1 - 0.5).abs() < 1e-6 && (p.exit.1 - 1.0).abs() < 1e-6);
        let p = t.phase_at(Ticks::from_millis(3_750));
        assert!((p.exit.1 - 0.5).abs() < 1e-6 && (p.enter.1 - 1.0).abs() < 1e-6);
        let p = t.phase_at(Ticks::from_seconds(2));
        assert_eq!((p.enter.1, p.exit.1), (1.0, 1.0));
        // A 3 s entrance on a 1 s text takes half the text.
        let mut short = TextItem::new("Hi", Ticks::ZERO, Ticks::SECOND);
        short.enter.duration = Ticks::from_seconds(3);
        assert!((short.phase_at(Ticks::from_millis(250)).enter.1 - 0.5).abs() < 1e-6);
        // Cuts are always fully shown.
        short.enter.kind = TextMotion::Cut;
        assert_eq!(short.phase_at(Ticks::ZERO).enter.1, 1.0);
    }

    #[test]
    fn validation_rejects_out_of_range_values() {
        let ok = TextItem::new("Hi", Ticks::ZERO, Ticks::SECOND);
        assert!(ok.validate().is_ok());
        let mut t = ok.clone();
        t.duration = Ticks::from_millis(10);
        assert!(t.validate().is_err());
        let mut t = ok.clone();
        t.style.size = 5;
        assert!(t.validate().is_err());
        let mut t = ok;
        t.width = 10;
        assert!(t.validate().is_err());
    }

    #[test]
    fn catalogues_are_indexable_and_names_stable() {
        for (i, f) in Font::ALL.iter().enumerate() {
            assert_eq!(Font::from_index(i), *f);
        }
        for (i, m) in TextMotion::ALL.iter().enumerate() {
            assert_eq!(TextMotion::from_index(i), *m);
        }
        assert_eq!(
            serde_json::to_string(&Font::PlayfairDisplay).unwrap(),
            "\"playfair_display\""
        );
        assert_eq!(
            serde_json::to_string(&TextMotion::SlideUp).unwrap(),
            "\"slide_up\""
        );
        let t: TextStyle = serde_json::from_str("{}").unwrap();
        assert_eq!(t, TextStyle::default());
    }
}
