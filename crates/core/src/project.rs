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

/// Kind of transition into a clip. More kinds arrive in Milestone 4.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    #[default]
    Cut,
    CrossDissolve,
    FadeThroughBlack,
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
        }
    }

    /// Time the clip occupies on the timeline (before transition overlap).
    #[must_use]
    pub fn duration(&self) -> Ticks {
        match self.source {
            ClipSource::Photo { duration } => duration,
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
}

impl Default for ProjectSettings {
    fn default() -> Self {
        ProjectSettings {
            aspect: Aspect::default(),
            frame_rate: FrameRate::default(),
            default_photo_duration: Ticks::from_seconds(4),
            default_transition: Transition::default(),
            default_fit: Fit::default(),
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
}

impl Default for Project {
    fn default() -> Self {
        Project {
            version: crate::PROJECT_FORMAT_VERSION,
            name: String::new(),
            settings: ProjectSettings::default(),
            media: BTreeMap::new(),
            clips: Vec::new(),
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

    /// Drops media references no clip uses any more.
    pub fn prune_media(&mut self) {
        let used: std::collections::BTreeSet<MediaId> =
            self.clips.iter().map(|c| c.media).collect();
        self.media.retain(|id, _| used.contains(id));
    }

    /// Checks structural invariants. Used by tests and after loading.
    pub fn validate(&self) -> Result<(), String> {
        for (i, clip) in self.clips.iter().enumerate() {
            if !self.media.contains_key(&clip.media) {
                return Err(format!("clip {i} references unknown media {}", clip.media));
            }
            match clip.source {
                ClipSource::Photo { duration } if duration <= Ticks::ZERO => {
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
                ClipSource::Photo { .. } => {}
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
        Ok(())
    }
}
