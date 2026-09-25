//! The project document: settings, referenced media and the clip sequence.
//!
//! Transitions live on the clip they lead *into* (`transition_in`), so
//! inserting or removing a clip never leaves a dangling transition. The
//! first clip's `transition_in` is applied from black.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::MediaId;
use crate::music::Music;
use crate::settings::Aspect;
use crate::time::{FrameRate, Ticks};

/// Identity of a clip on the timeline. Survives reordering and undo.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClipId(Uuid);

impl ClipId {
    #[must_use]
    pub fn new() -> Self {
        ClipId(Uuid::new_v4())
    }
}

impl Default for ClipId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ClipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

impl FromStr for ClipId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(ClipId)
    }
}

/// What kind of media a reference points to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Photo,
    Video,
    Audio,
}

/// Everything the project needs to know about a media item without asking
/// the library, so a project file is self-describing and relinkable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRef {
    pub id: MediaId,
    pub kind: RefKind,
    /// Last known location, for relinking and display.
    pub path: PathBuf,
    /// Content fingerprint (hash) and size, matching the library's.
    pub fingerprint_hash: u64,
    pub size: u64,
    /// Displayed pixel size (after rotation). `None` for audio.
    #[serde(default)]
    pub pixel_size: Option<(u32, u32)>,
    /// Natural duration for video and audio.
    #[serde(default)]
    pub duration: Option<Ticks>,
    /// Capture time in Unix milliseconds, for sorting by date.
    #[serde(default)]
    pub captured_at_ms: Option<i64>,
    /// Display name (file name at link time).
    #[serde(default)]
    pub name: String,
}

/// How a picture is placed into the frame.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Whole picture visible, bars where the aspect differs.
    #[default]
    Contain,
    /// Frame filled, picture cropped where the aspect differs.
    Cover,
}

/// Quarter-turn rotation applied on top of the file's own orientation.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quarter {
    #[default]
    None,
    Cw90,
    Cw180,
    Cw270,
}

impl Quarter {
    #[must_use]
    pub const fn next(self) -> Quarter {
        match self {
            Quarter::None => Quarter::Cw90,
            Quarter::Cw90 => Quarter::Cw180,
            Quarter::Cw180 => Quarter::Cw270,
            Quarter::Cw270 => Quarter::None,
        }
    }

    #[must_use]
    pub const fn swaps_dimensions(self) -> bool {
        matches!(self, Quarter::Cw90 | Quarter::Cw270)
    }
}

/// Kind of transition into a clip. Serialised names are stable; new kinds
/// are appended, never renamed.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    #[default]
    Cut,
    CrossDissolve,
    FadeThroughBlack,
    FadeThroughWhite,
    /// The incoming clip pushes the outgoing one out towards the left.
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    /// A straight edge moving to the left reveals the incoming clip.
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    /// The incoming clip grows from the centre while fading in.
    Zoom,
}

impl TransitionKind {
    /// Every kind, in the order the transition picker lists them.
    pub const ALL: [TransitionKind; 13] = [
        TransitionKind::Cut,
        TransitionKind::CrossDissolve,
        TransitionKind::FadeThroughBlack,
        TransitionKind::FadeThroughWhite,
        TransitionKind::SlideLeft,
        TransitionKind::SlideRight,
        TransitionKind::SlideUp,
        TransitionKind::SlideDown,
        TransitionKind::WipeLeft,
        TransitionKind::WipeRight,
        TransitionKind::WipeUp,
        TransitionKind::WipeDown,
        TransitionKind::Zoom,
    ];

    /// Kinds the "shuffle" action picks from: everything but the cut.
    pub const SHUFFLE_POOL: [TransitionKind; 12] = [
        TransitionKind::CrossDissolve,
        TransitionKind::FadeThroughBlack,
        TransitionKind::FadeThroughWhite,
        TransitionKind::SlideLeft,
        TransitionKind::SlideRight,
        TransitionKind::SlideUp,
        TransitionKind::SlideDown,
        TransitionKind::WipeLeft,
        TransitionKind::WipeRight,
        TransitionKind::WipeUp,
        TransitionKind::WipeDown,
        TransitionKind::Zoom,
    ];

    /// Position in [`TransitionKind::ALL`]; used as the picker index.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// Kind for a picker index; out-of-range means `Cut`.
    #[must_use]
    pub fn from_index(index: usize) -> TransitionKind {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// Transitions that pass through a solid colour between the clips.
    #[must_use]
    pub const fn is_fade(self) -> bool {
        matches!(
            self,
            TransitionKind::FadeThroughBlack | TransitionKind::FadeThroughWhite
        )
    }
}

/// Slow camera movement over a photo ("Ken Burns"). The movement spans
/// the whole time the clip is visible, including transition overlaps.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    #[default]
    None,
    ZoomIn,
    ZoomOut,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
}

impl Motion {
    /// Every preset, in picker order.
    pub const ALL: [Motion; 7] = [
        Motion::None,
        Motion::ZoomIn,
        Motion::ZoomOut,
        Motion::PanLeft,
        Motion::PanRight,
        Motion::PanUp,
        Motion::PanDown,
    ];

    /// Presets the shuffle action picks from.
    pub const SHUFFLE_POOL: [Motion; 6] = [
        Motion::ZoomIn,
        Motion::ZoomOut,
        Motion::PanLeft,
        Motion::PanRight,
        Motion::PanUp,
        Motion::PanDown,
    ];

    /// How much larger than the frame the picture is at the widest point of
    /// the movement. 1.15 is noticeable but calm over four seconds.
    pub const SCALE: f64 = 1.15;

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    #[must_use]
    pub fn from_index(index: usize) -> Motion {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// Camera state at `progress` (0..=1 over the clip): zoom factor (>= 1)
    /// and the offset of the visible window's centre from the picture's
    /// centre, as a fraction of the frame size in `-0.5..=0.5`. Eased so the
    /// movement starts and stops softly.
    #[must_use]
    pub fn camera(self, progress: f64) -> (f64, f64, f64) {
        let t = progress.clamp(0.0, 1.0);
        // Smoothstep easing.
        let e = t * t * (3.0 - 2.0 * t);
        // Maximum pan that keeps the zoomed picture covering the frame.
        let travel = (Self::SCALE - 1.0) / (2.0 * Self::SCALE);
        match self {
            Motion::None => (1.0, 0.0, 0.0),
            Motion::ZoomIn => (1.0 + (Self::SCALE - 1.0) * e, 0.0, 0.0),
            Motion::ZoomOut => (Self::SCALE - (Self::SCALE - 1.0) * e, 0.0, 0.0),
            Motion::PanLeft => (Self::SCALE, travel * (1.0 - 2.0 * e), 0.0),
            Motion::PanRight => (Self::SCALE, -travel * (1.0 - 2.0 * e), 0.0),
            Motion::PanUp => (Self::SCALE, 0.0, travel * (1.0 - 2.0 * e)),
            Motion::PanDown => (Self::SCALE, 0.0, -travel * (1.0 - 2.0 * e)),
        }
    }
}

/// Look of a caption or title text. Serialised names are stable.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionStyle {
    /// White text with a soft shadow, centred near the bottom.
    #[default]
    Classic,
    /// Text on a dark translucent band across the bottom.
    Banner,
    /// Large bold text in the centre; further lines smaller. Title cards.
    Headline,
    /// Small text in the bottom-left corner.
    Corner,
}

impl CaptionStyle {
    pub const ALL: [CaptionStyle; 4] = [
        CaptionStyle::Classic,
        CaptionStyle::Banner,
        CaptionStyle::Headline,
        CaptionStyle::Corner,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    #[must_use]
    pub fn from_index(index: usize) -> CaptionStyle {
        Self::ALL.get(index).copied().unwrap_or_default()
    }
}

/// Text shown over a clip for as long as the clip is visible. Line breaks
/// in `text` are kept.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Caption {
    pub text: String,
    #[serde(default)]
    pub style: CaptionStyle,
}

impl Caption {
    #[must_use]
    pub fn new(text: impl Into<String>, style: CaptionStyle) -> Caption {
        Caption {
            text: text.into(),
            style,
        }
    }
}

/// Background colour of a title card.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleBackground {
    #[default]
    Black,
    Charcoal,
    Blue,
    Red,
    White,
}

impl TitleBackground {
    pub const ALL: [TitleBackground; 5] = [
        TitleBackground::Black,
        TitleBackground::Charcoal,
        TitleBackground::Blue,
        TitleBackground::Red,
        TitleBackground::White,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(0)
    }

    #[must_use]
    pub fn from_index(index: usize) -> TitleBackground {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// Light backgrounds need dark text.
    #[must_use]
    pub const fn is_light(self) -> bool {
        matches!(self, TitleBackground::White)
    }
}

/// A transition from the previous clip into this one.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    /// Overlap with the previous clip. Ignored (treated as zero) for `Cut`.
    pub duration: Ticks,
}

impl Transition {
    pub const CUT: Transition = Transition {
        kind: TransitionKind::Cut,
        duration: Ticks::ZERO,
    };

    /// How much this transition overlaps the previous clip.
    #[must_use]
    pub fn overlap(self) -> Ticks {
        match self.kind {
            TransitionKind::Cut => Ticks::ZERO,
            _ => self.duration.max(Ticks::ZERO),
        }
    }
}

impl Default for Transition {
    fn default() -> Self {
        Self::CUT
    }
}

/// Where a clip's frames come from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipSource {
    /// A still shown for `duration`.
    Photo { duration: Ticks },
    /// A video trimmed to `in_point..out_point` of the source.
    Video { in_point: Ticks, out_point: Ticks },
    /// A title card: a solid background for `duration`; the text is the
    /// clip's caption.
    Title {
        duration: Ticks,
        #[serde(default)]
        background: TitleBackground,
    },
}

/// One item on the video track.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub media: MediaId,
    pub source: ClipSource,
    #[serde(default)]
    pub fit: Fit,
    #[serde(default)]
    pub rotate: Quarter,
    #[serde(default)]
    pub transition_in: Transition,
    #[serde(default)]
    pub muted: bool,
    /// Audio gain in percent (100 = unchanged). Only meaningful for video.
    #[serde(default = "default_volume")]
    pub volume_percent: u16,
    /// Ken Burns movement. Only applied to photos.
    #[serde(default)]
    pub motion: Motion,
    /// Text over the clip (the title of a title card).
    #[serde(default)]
    pub caption: Option<Caption>,
}

fn default_volume() -> u16 {
    100
}

impl Clip {
    /// A photo clip with the given duration.
    #[must_use]
    pub fn photo(media: MediaId, duration: Ticks) -> Clip {
        Clip {
            id: ClipId::new(),
            media,
            source: ClipSource::Photo { duration },
            fit: Fit::default(),
            rotate: Quarter::default(),
            transition_in: Transition::default(),
            muted: false,
            volume_percent: 100,
            motion: Motion::None,
            caption: None,
        }
    }

    /// A title card showing `text` (headline style) on `background`.
    #[must_use]
    pub fn title(text: impl Into<String>, background: TitleBackground, duration: Ticks) -> Clip {
        Clip {
            id: ClipId::new(),
            media: MediaId::NONE,
            source: ClipSource::Title {
                duration,
                background,
            },
            fit: Fit::default(),
            rotate: Quarter::default(),
            transition_in: Transition::default(),
            muted: false,
            volume_percent: 100,
            motion: Motion::None,
            caption: Some(Caption::new(text, CaptionStyle::Headline)),
        }
    }

    /// A video clip using the whole source.
    #[must_use]
    pub fn video(media: MediaId, natural_duration: Ticks) -> Clip {
        Clip {
            id: ClipId::new(),
            media,
            source: ClipSource::Video {
                in_point: Ticks::ZERO,
                out_point: natural_duration,
            },
            fit: Fit::default(),
            rotate: Quarter::default(),
            transition_in: Transition::default(),
            muted: false,
            volume_percent: 100,
            motion: Motion::None,
            caption: None,
        }
    }

    /// Linear gain factor from `volume_percent`, 0 when muted.
    #[must_use]
    pub fn gain(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            f32::from(self.volume_percent.min(300)) / 100.0
        }
    }

    /// Time the clip occupies on the timeline (before transition overlap).
    #[must_use]
    pub fn duration(&self) -> Ticks {
        match self.source {
            ClipSource::Photo { duration } | ClipSource::Title { duration, .. } => duration,
            ClipSource::Video {
                in_point,
                out_point,
            } => (out_point - in_point).max(Ticks::ZERO),
        }
    }

    #[must_use]
    pub const fn is_photo(&self) -> bool {
        matches!(self.source, ClipSource::Photo { .. })
    }

    #[must_use]
    pub const fn is_title(&self) -> bool {
        matches!(self.source, ClipSource::Title { .. })
    }

    #[must_use]
    pub const fn is_video(&self) -> bool {
        matches!(self.source, ClipSource::Video { .. })
    }

    /// Photos and title cards: clips with a free duration.
    #[must_use]
    pub const fn is_still(&self) -> bool {
        !self.is_video()
    }
}

/// Project-wide settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub aspect: Aspect,
    pub frame_rate: FrameRate,
    /// Duration new photos get when added.
    pub default_photo_duration: Ticks,
    /// Transition new clips get when added.
    #[serde(default)]
    pub default_transition: Transition,
    /// Fit new clips get when added.
    #[serde(default)]
    pub default_fit: Fit,
    /// Ken Burns movement new photos get when added.
    #[serde(default)]
    pub default_motion: Motion,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        ProjectSettings {
            aspect: Aspect::default(),
            frame_rate: FrameRate::default(),
            default_photo_duration: Ticks::from_seconds(4),
            default_transition: Transition::default(),
            default_fit: Fit::default(),
            default_motion: Motion::None,
        }
    }
}

/// The document. Mutated only through [`crate::command::Command`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    #[serde(default)]
    pub name: String,
    pub settings: ProjectSettings,
    /// Media referenced by clips (and possibly some no longer referenced;
    /// pruned on save).
    #[serde(default)]
    pub media: BTreeMap<MediaId, MediaRef>,
    #[serde(default)]
    pub clips: Vec<Clip>,
    /// Background music.
    #[serde(default)]
    pub music: Music,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            version: crate::PROJECT_FORMAT_VERSION,
            name: String::new(),
            settings: ProjectSettings::default(),
            media: BTreeMap::new(),
            clips: Vec::new(),
            music: Music::default(),
        }
    }
}

impl Project {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn media_ref(&self, id: MediaId) -> Option<&MediaRef> {
        self.media.get(&id)
    }

    /// Index of the clip with `id`.
    #[must_use]
    pub fn index_of(&self, id: ClipId) -> Option<usize> {
        self.clips.iter().position(|c| c.id == id)
    }

    /// Drops media references no clip or song uses any more.
    pub fn prune_media(&mut self) {
        let used: std::collections::BTreeSet<MediaId> = self
            .clips
            .iter()
            .map(|c| c.media)
            .chain(self.music.songs.iter().map(|s| s.media))
            .collect();
        self.media.retain(|id, _| used.contains(id));
    }

    /// Checks structural invariants. Used by tests and after loading.
    pub fn validate(&self) -> Result<(), String> {
        for (i, clip) in self.clips.iter().enumerate() {
            if !clip.is_title() && !self.media.contains_key(&clip.media) {
                return Err(format!("clip {i} references unknown media {}", clip.media));
            }
            match clip.source {
                ClipSource::Photo { duration } | ClipSource::Title { duration, .. }
                    if duration <= Ticks::ZERO =>
                {
                    return Err(format!("clip {i} has non-positive duration"));
                }
                ClipSource::Video {
                    in_point,
                    out_point,
                } => {
                    if in_point < Ticks::ZERO || out_point <= in_point {
                        return Err(format!("clip {i} has an invalid trim range"));
                    }
                    if let Some(natural) = self.media.get(&clip.media).and_then(|m| m.duration)
                        && out_point > natural
                    {
                        return Err(format!("clip {i} out point exceeds source duration"));
                    }
                }
                ClipSource::Photo { .. } | ClipSource::Title { .. } => {}
            }
            if clip.transition_in.duration < Ticks::ZERO {
                return Err(format!("clip {i} has a negative transition"));
            }
        }
        let mut ids: Vec<ClipId> = self.clips.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        if ids.len() != self.clips.len() {
            return Err("duplicate clip ids".to_owned());
        }
        self.validate_music()
    }

    /// Checks the music track alone (used by `SetMusic`).
    pub fn validate_music(&self) -> Result<(), String> {
        let music = &self.music;
        for (i, song) in music.songs.iter().enumerate() {
            if !self.media.contains_key(&song.media) {
                return Err(format!("song {i} references unknown media {}", song.media));
            }
        }
        let mut ids: Vec<crate::music::SongId> = music.songs.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        if ids.len() != music.songs.len() {
            return Err("duplicate song ids".to_owned());
        }
        if music.volume_percent > Music::MAX_VOLUME {
            return Err("music volume out of range".to_owned());
        }
        if music.fade_in < Ticks::ZERO || music.fade_out < Ticks::ZERO {
            return Err("negative music fade".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_catalogue_is_complete_and_indexable() {
        for (i, k) in TransitionKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i);
            assert_eq!(TransitionKind::from_index(i), *k);
        }
        assert_eq!(TransitionKind::from_index(999), TransitionKind::Cut);
        assert!(!TransitionKind::SHUFFLE_POOL.contains(&TransitionKind::Cut));
        assert_eq!(
            TransitionKind::SHUFFLE_POOL.len() + 1,
            TransitionKind::ALL.len()
        );
        assert!(TransitionKind::FadeThroughWhite.is_fade() && !TransitionKind::WipeLeft.is_fade());
    }

    #[test]
    fn transition_names_are_stable_on_disk() {
        let json = serde_json::to_string(&TransitionKind::FadeThroughWhite).unwrap();
        assert_eq!(json, "\"fade_through_white\"");
        assert_eq!(
            serde_json::to_string(&TransitionKind::SlideLeft).unwrap(),
            "\"slide_left\""
        );
        assert_eq!(serde_json::to_string(&Motion::PanUp).unwrap(), "\"pan_up\"");
    }

    #[test]
    fn camera_starts_and_ends_at_the_preset_extremes() {
        assert_eq!(Motion::None.camera(0.3), (1.0, 0.0, 0.0));
        let (z0, ..) = Motion::ZoomIn.camera(0.0);
        let (z1, ..) = Motion::ZoomIn.camera(1.0);
        assert!((z0 - 1.0).abs() < 1e-12 && (z1 - Motion::SCALE).abs() < 1e-12);
        let (z0, ..) = Motion::ZoomOut.camera(0.0);
        assert!((z0 - Motion::SCALE).abs() < 1e-12);
        // Pans travel from one side to the other and pass the centre half way.
        let (_, x0, _) = Motion::PanLeft.camera(0.0);
        let (_, xm, _) = Motion::PanLeft.camera(0.5);
        let (_, x1, _) = Motion::PanLeft.camera(1.0);
        assert!(x0 > 0.0 && xm.abs() < 1e-12 && x1 < 0.0);
        assert!((x0 + x1).abs() < 1e-12, "symmetric travel");
        // The zoomed picture always covers the frame: |offset| <= (s-1)/(2s).
        for m in Motion::ALL {
            for i in 0..=20 {
                let (z, x, y) = m.camera(f64::from(i) / 20.0);
                assert!(z >= 1.0);
                let limit = (z - 1.0) / (2.0 * z) + 1e-12;
                assert!(
                    x.abs() <= limit && y.abs() <= limit,
                    "{m:?} at {i}: {z} {x} {y}"
                );
            }
        }
        // Clamped outside 0..1 and eased (slow at the ends).
        assert_eq!(Motion::ZoomIn.camera(-1.0), Motion::ZoomIn.camera(0.0));
        let (a, ..) = Motion::ZoomIn.camera(0.05);
        let (b, ..) = Motion::ZoomIn.camera(0.5);
        assert!(a - 1.0 < (b - 1.0) * 0.1, "ease-in");
    }

    #[test]
    fn old_project_files_default_the_new_fields() {
        let json = r#"{"id":"00000000-0000-0000-0000-000000000001","media":"00000000-0000-0000-0000-000000000002","source":{"photo":{"duration":705600000}}}"#;
        let clip: Clip = serde_json::from_str(json).unwrap();
        assert_eq!(clip.motion, Motion::None);
        assert_eq!(clip.volume_percent, 100);
    }
}
