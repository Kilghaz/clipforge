//! Project file format: JSON with a version number and forward-compatible
//! parsing (unknown fields are ignored, missing new fields get defaults).

use crate::PROJECT_FORMAT_VERSION;
use crate::project::Project;

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("not a ClipForge project: {0}")]
    Parse(#[from] serde_json::Error),
    #[error(
        "project was saved by a newer ClipForge (format {found}, this build reads up to {supported})"
    )]
    TooNew { found: u32, supported: u32 },
    #[error("project is inconsistent: {0}")]
    Invalid(String),
}

impl Project {
    /// Serialises to pretty JSON. Unused media references are dropped.
    pub fn to_json(&self) -> Result<String, PersistError> {
        let mut copy = self.clone();
        copy.version = PROJECT_FORMAT_VERSION;
        copy.prune_media();
        Ok(serde_json::to_string_pretty(&copy)?)
    }

    /// Parses and validates a project file.
    pub fn from_json(json: &str) -> Result<Project, PersistError> {
        let mut value: serde_json::Value = serde_json::from_str(json)?;
        let captions = take_legacy_captions(&mut value);
        let mut project: Project = serde_json::from_value(value)?;
        captions_to_texts(&mut project, captions);
        if project.version > PROJECT_FORMAT_VERSION {
            return Err(PersistError::TooNew {
                found: project.version,
                supported: PROJECT_FORMAT_VERSION,
            });
        }
        let project = migrate(project);
        project.validate().map_err(PersistError::Invalid)?;
        Ok(project)
    }
}

/// A caption as the milestone-5 preview stored it on a clip.
struct LegacyCaption {
    clip: usize,
    text: String,
    style: String,
}

/// Removes `caption` from every clip (the field was replaced by the text
/// track) and returns what was there.
fn take_legacy_captions(value: &mut serde_json::Value) -> Vec<LegacyCaption> {
    let mut out = Vec::new();
    let Some(clips) = value.get_mut("clips").and_then(|c| c.as_array_mut()) else {
        return out;
    };
    for (i, clip) in clips.iter_mut().enumerate() {
        let Some(obj) = clip.as_object_mut() else {
            continue;
        };
        let Some(caption) = obj.remove("caption") else {
            continue;
        };
        let text = caption.get("text").and_then(|t| t.as_str()).unwrap_or("");
        if text.trim().is_empty() {
            continue;
        }
        out.push(LegacyCaption {
            clip: i,
            text: text.to_owned(),
            style: caption
                .get("style")
                .and_then(|s| s.as_str())
                .unwrap_or("classic")
                .to_owned(),
        });
    }
    out
}

/// Turns old per-clip captions into text items over the same span, with a
/// look close to the old preset.
fn captions_to_texts(project: &mut Project, captions: Vec<LegacyCaption>) {
    use crate::text::{TextAlign, TextItem};
    let places = crate::timeline::placements(&project.clips);
    for c in captions {
        let Some(place) = places.get(c.clip) else {
            continue;
        };
        let mut t = TextItem::new(c.text, place.start, place.duration());
        match c.style.as_str() {
            "headline" => {
                t.style.size = 1_000;
                t.style.bold = true;
            }
            "banner" => {
                t.y = 8_600;
                t.width = 9_000;
                t.style.size = 440;
                t.style.shadow = false;
                t.style.background = Some([0, 0, 0, 150]);
            }
            "corner" => {
                t.x = 2_800;
                t.y = 9_000;
                t.width = 5_000;
                t.style.size = 340;
                t.style.align = TextAlign::Left;
            }
            _ => {
                t.y = 8_500;
                t.style.size = 520;
                t.style.bold = true;
            }
        }
        if t.validate().is_ok() {
            project.texts.push(t);
        }
    }
}

/// Upgrades older documents in place. Nothing to do for version 1.
fn migrate(mut project: Project) -> Project {
    project.version = PROJECT_FORMAT_VERSION;
    project
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::ids::MediaId;
    use crate::project::{Clip, MediaRef, RefKind};
    use crate::time::Ticks;

    fn sample() -> Project {
        let mut p = Project::new();
        p.name = "Holiday".into();
        let m = MediaRef {
            id: MediaId::new(),
            kind: RefKind::Photo,
            path: "/photos/a.jpg".into(),
            fingerprint_hash: 42,
            size: 1234,
            pixel_size: Some((4032, 3024)),
            duration: None,
            captured_at_ms: Some(1_700_000_000_000),
            name: "a.jpg".into(),
        };
        let unused = MediaRef {
            id: MediaId::new(),
            ..m.clone()
        };
        Command::InsertClips {
            entries: vec![(0, Clip::photo(m.id, Ticks::from_seconds(3)))],
            media: vec![m, unused],
        }
        .apply(&mut p)
        .unwrap();
        p
    }

    #[test]
    fn round_trips_and_prunes_unused_media() {
        let p = sample();
        assert_eq!(p.media.len(), 2);
        let json = p.to_json().unwrap();
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back.media.len(), 1);
        assert_eq!(back.clips, p.clips);
        assert_eq!(back.name, "Holiday");
        assert!(json.contains("\"version\": 1"));
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_fields_defaulted() {
        let json = r#"{"version":1,"settings":{"aspect":"landscape16x9","frame_rate":{"num":30,"den":1},"default_photo_duration":2822400000},"future":true}"#;
        let p = Project::from_json(json).unwrap();
        assert!(p.clips.is_empty());
        assert_eq!(p.settings.default_photo_duration, Ticks::from_seconds(4));
    }

    #[test]
    fn rejects_newer_and_invalid_files() {
        let mut p = sample();
        p.version = 99;
        let json = serde_json::to_string(&p).unwrap();
        assert!(matches!(
            Project::from_json(&json),
            Err(PersistError::TooNew { found: 99, .. })
        ));

        let mut p = sample();
        p.clips[0].media = MediaId::new();
        let json = serde_json::to_string(&p).unwrap();
        assert!(matches!(
            Project::from_json(&json),
            Err(PersistError::Invalid(_))
        ));

        assert!(matches!(
            Project::from_json("{not json"),
            Err(PersistError::Parse(_))
        ));
    }

    #[test]
    fn old_captions_become_texts_on_load() {
        let mut p = sample();
        let m = p.media.values().next().unwrap().id;
        Command::InsertClips {
            entries: vec![(1, Clip::photo(m, Ticks::from_seconds(4)))],
            media: vec![],
        }
        .apply(&mut p)
        .unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
        value["clips"][1]["caption"] = serde_json::json!({"text": "Rome", "style": "banner"});
        value["clips"][0]["caption"] = serde_json::json!({"text": "  ", "style": "classic"});
        let back = Project::from_json(&value.to_string()).unwrap();
        assert_eq!(back.texts.len(), 1, "empty captions are dropped");
        let t = &back.texts[0];
        assert_eq!(t.text, "Rome");
        assert_eq!(
            (t.start, t.duration),
            (Ticks::from_seconds(3), Ticks::from_seconds(4))
        );
        assert!(t.style.background.is_some());
        assert_eq!(back.clips, p.clips);
    }
}
