//! Persisted user settings.

use std::path::PathBuf;
use std::sync::Arc;

use clipforge_i18n::LanguagePreference;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Everything the user can change in the Settings page.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub language: LanguagePreference,
}

/// Loads and saves [`Settings`] as JSON at a fixed path.
#[derive(Clone, Debug)]
pub(crate) struct SettingsStore {
    path: Arc<PathBuf>,
}

impl SettingsStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        SettingsStore {
            path: Arc::new(path),
        }
    }

    /// Reads settings. A missing or unreadable file yields defaults, so a
    /// corrupt settings file can never prevent the app from starting.
    pub(crate) fn load(&self) -> Settings {
        match std::fs::read(&*self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                warn!(path = %self.path.display(), error = %e, "settings file unreadable, using defaults");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    /// Writes atomically (temp file + rename) so a crash mid-write leaves the
    /// previous settings intact.
    pub(crate) fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
        std::fs::rename(&tmp, &*self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_i18n::Language;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("nested").join("settings.json"));
        assert_eq!(store.load(), Settings::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("nested").join("settings.json"));
        let s = Settings {
            language: LanguagePreference::Fixed(Language::German),
        };
        store.save(&s).unwrap();
        assert_eq!(store.load(), s);
        assert!(!dir.path().join("nested").join("settings.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(SettingsStore::new(path).load(), Settings::default());
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_fields_defaulted() {
        let s: Settings = serde_json::from_str(r#"{"future_option": 1}"#).unwrap();
        assert_eq!(s, Settings::default());
    }
}
