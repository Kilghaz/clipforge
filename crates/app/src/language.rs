//! Mapping between the language picker and the bundled Slint translations.

use clipforge_i18n::{Language, LanguagePreference};
use slint::SharedString;
use tracing::warn;

/// Entries of the language picker: "System" first, then every shipped
/// language by its native name.
pub(crate) fn picker_options(system_label: &str) -> Vec<SharedString> {
    let mut options = vec![SharedString::from(system_label)];
    options.extend(
        Language::ALL
            .iter()
            .map(|l| SharedString::from(l.native_name())),
    );
    options
}

/// Picker index for a stored preference.
pub(crate) fn picker_index(pref: LanguagePreference) -> i32 {
    match pref {
        LanguagePreference::System => 0,
        LanguagePreference::Fixed(lang) => {
            let pos = Language::ALL.iter().position(|l| *l == lang).unwrap_or(0);
            i32::try_from(pos + 1).unwrap_or(0)
        }
    }
}

/// Preference for a picker index. Out-of-range indices mean "System".
pub(crate) fn picker_preference(index: i32) -> LanguagePreference {
    usize::try_from(index)
        .ok()
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| Language::ALL.get(i).copied())
        .map_or(LanguagePreference::System, LanguagePreference::Fixed)
}

/// Switches the running UI to the given preference.
pub(crate) fn apply(pref: LanguagePreference) {
    let lang = pref.resolve();
    if let Err(e) = slint::select_bundled_translation(lang.code()) {
        warn!(language = lang.code(), error = %e, "could not select translation");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_preference_round_trip() {
        for pref in [
            LanguagePreference::System,
            LanguagePreference::Fixed(Language::English),
            LanguagePreference::Fixed(Language::German),
        ] {
            assert_eq!(picker_preference(picker_index(pref)), pref);
        }
        assert_eq!(picker_preference(-1), LanguagePreference::System);
        assert_eq!(picker_preference(99), LanguagePreference::System);
    }

    #[test]
    fn picker_lists_system_then_native_names() {
        let opts = picker_options("Systemsprache");
        assert_eq!(opts, ["Systemsprache", "English", "Deutsch"]);
    }
}
