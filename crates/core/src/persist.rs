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
        let project: Project = serde_json::from_str(json)?;
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
}
