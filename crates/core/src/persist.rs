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
        sizes_to_points(&mut value);
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

/// The first text-track format stored the size as 1/10 000 of the short
/// side (`size`); now it is points on a 1080-line frame (`points`).
fn sizes_to_points(value: &mut serde_json::Value) {
    let Some(texts) = value.get_mut("texts").and_then(|t| t.as_array_mut()) else {
        return;
    };
    for text in texts {
        let Some(style) = text.get_mut("style").and_then(|s| s.as_object_mut()) else {
            continue;
        };
        if style.contains_key("points") {
            continue;
        }
        if let Some(size) = style.remove("size").and_then(|v| v.as_u64()) {
            let points =
                (size * u64::from(crate::text::TextStyle::REFERENCE_LINES) + 5_000) / 10_000;
            style.insert("points".to_owned(), serde_json::Value::from(points));
        }
    }
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
                t.style.points = 108;
                t.style.bold = true;
            }
            "banner" => {
                t.y = 8_600;
                t.width = 9_000;
                t.style.points = 48;
                t.style.shadow = false;
                t.style.background = Some([0, 0, 0, 150]);
            }
            "corner" => {
                t.x = 2_800;
                t.y = 9_000;
                t.width = 5_000;
                t.style.points = 37;
                t.style.align = TextAlign::Left;
            }
            _ => {
                t.y = 8_500;
                t.style.points = 56;
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
            hdr: false,
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

    #[test]
    fn first_text_format_sizes_become_points() {
        let mut p = sample();
        Command::InsertTexts {
            entries: vec![(
                0,
                crate::text::TextItem::new("Hi", Ticks::ZERO, Ticks::SECOND),
            )],
        }
        .apply(&mut p)
        .unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
        let style = value["texts"][0]["style"].as_object_mut().unwrap();
        style.remove("points");
        style.insert("size".into(), serde_json::json!(600));
        style.insert("font".into(), serde_json::json!("caveat"));
        let back = Project::from_json(&value.to_string()).unwrap();
        assert_eq!(back.texts[0].style.points, 65);
        assert_eq!(back.texts[0].style.font, "Caveat");
    }
}
