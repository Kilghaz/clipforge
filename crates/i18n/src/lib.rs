//! Languages ClipForge speaks.
//!
//! Strings themselves are translated by Slint's bundled `@tr()` catalogues
//! in the `app` crate. This crate only knows which languages exist, how to
//! detect the system language and how to map a BCP 47 tag to one of ours.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// A UI language shipped with the app.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    English,
    German,
}

impl Language {
    /// All shipped languages in menu order.
    pub const ALL: [Language; 2] = [Language::English, Language::German];

    /// Language code used for the bundled translation catalogue.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::German => "de",
        }
    }

    /// Name of the language in that language, for the language picker.
    #[must_use]
    pub const fn native_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::German => "Deutsch",
        }
    }

    /// Maps a BCP 47 tag or POSIX locale (`de`, `de-AT`, `de_DE.UTF-8`,
    /// `en-US`) to a shipped language. Unknown languages map to `None`.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Language> {
        let primary = tag.split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Language::English),
            "de" => Some(Language::German),
            _ => None,
        }
    }

    /// Best match for the operating system's preferred languages, or English.
    #[must_use]
    pub fn system() -> Language {
        sys_locale::get_locales()
            .find_map(|tag| Language::from_tag(&tag))
            .unwrap_or_default()
    }
}

/// The user's language preference as stored in settings.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    #[default]
    System,
    Fixed(Language),
}

impl LanguagePreference {
    /// The language to actually use.
    #[must_use]
    pub fn resolve(self) -> Language {
        match self {
            LanguagePreference::System => Language::system(),
            LanguagePreference::Fixed(lang) => lang,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_in_all_common_shapes() {
        for tag in ["de", "DE", "de-DE", "de_AT", "de_DE.UTF-8", "de-CH@euro"] {
            assert_eq!(Language::from_tag(tag), Some(Language::German), "{tag}");
        }
        for tag in ["en", "en-US", "en_GB.UTF-8"] {
            assert_eq!(Language::from_tag(tag), Some(Language::English), "{tag}");
        }
        assert_eq!(Language::from_tag("fr-FR"), None);
        assert_eq!(Language::from_tag(""), None);
    }

    #[test]
    fn codes_are_unique_and_lowercase() {
        let codes: Vec<_> = Language::ALL.iter().map(|l| l.code()).collect();
        assert_eq!(codes, ["en", "de"]);
        for l in Language::ALL {
            assert_eq!(Language::from_tag(l.code()), Some(l));
        }
    }

    #[test]
    fn system_never_panics_and_fixed_wins() {
        let _ = Language::system();
        assert_eq!(
            LanguagePreference::Fixed(Language::German).resolve(),
            Language::German
        );
    }

    #[test]
    fn preference_serializes_readably() {
        assert_eq!(
            serde_json::to_string(&LanguagePreference::System).unwrap(),
            "\"system\""
        );
        assert_eq!(
            serde_json::to_string(&LanguagePreference::Fixed(Language::German)).unwrap(),
            "{\"fixed\":\"german\"}"
        );
    }
}
