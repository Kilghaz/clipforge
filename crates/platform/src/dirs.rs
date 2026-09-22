//! Where ClipForge keeps its own files.

use std::path::{Path, PathBuf};

/// Locations for configuration, the library catalogue and the cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDirs {
    /// Settings, recent projects.
    pub config: PathBuf,
    /// Library catalogue (SQLite) and other durable state.
    pub data: PathBuf,
    /// Thumbnails and proxies. Safe to delete at any time.
    pub cache: PathBuf,
}

impl AppDirs {
    /// Standard per-user locations for the current OS. `None` only if the
    /// home directory cannot be determined.
    #[must_use]
    pub fn standard() -> Option<AppDirs> {
        let dirs = directories::ProjectDirs::from("dev", "clipforge", "ClipForge")?;
        Some(AppDirs {
            config: dirs.config_dir().to_path_buf(),
            data: dirs.data_dir().to_path_buf(),
            cache: dirs.cache_dir().to_path_buf(),
        })
    }

    /// All three directories below one root. Used by tests and by the
    /// portable mode.
    #[must_use]
    pub fn under(root: &Path) -> AppDirs {
        AppDirs {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    /// Creates the directories if they do not exist.
    pub fn ensure_exist(&self) -> std::io::Result<()> {
        for dir in [&self.config, &self.data, &self.cache] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Path of the settings file.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_dirs_mention_the_app_name() {
        let dirs = AppDirs::standard().expect("home directory");
        for p in [&dirs.config, &dirs.data, &dirs.cache] {
            assert!(
                p.to_string_lossy().to_lowercase().contains("clipforge"),
                "{p:?}"
            );
        }
    }

    #[test]
    fn under_creates_three_distinct_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(tmp.path());
        dirs.ensure_exist().unwrap();
        assert!(dirs.config.is_dir() && dirs.data.is_dir() && dirs.cache.is_dir());
        assert_ne!(dirs.config, dirs.cache);
        assert_eq!(
            dirs.settings_file(),
            tmp.path().join("config").join("settings.json")
        );
    }
}
