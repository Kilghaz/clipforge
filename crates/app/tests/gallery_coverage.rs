//! Every exported UI component must appear in the gallery
//! (docs/ux/DESIGN.md §4): the gallery is the evidence store for spec cards,
//! and a component that is not in it has never been looked at in isolation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

fn ui(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Exported components in a file, excluding globals, structs and windows.
fn exported_components(src: &str) -> Vec<String> {
    let re =
        regex::Regex::new(r"(?m)^export component ([A-Za-z0-9_-]+) inherits ([A-Za-z]+)").unwrap();
    re.captures_iter(src)
        .filter(|c| &c[2] != "Window")
        .map(|c| c[1].to_owned())
        .collect()
}

#[test]
fn every_exported_component_has_a_gallery_row() {
    let gallery = ui("gallery.slint");
    // Screen-level pages are covered by the app scenes, not the gallery.
    let pages = ["LibraryPage", "EditorPage", "SettingsPage"];
    let mut missing = Vec::new();
    for file in ["components.slint", "library.slint", "editor.slint"] {
        for name in exported_components(&ui(file)) {
            if pages.contains(&name.as_str()) {
                continue;
            }
            let used = regex::Regex::new(&format!(r"\b{}\s*\{{", regex::escape(&name)))
                .unwrap()
                .is_match(&gallery);
            if !used {
                missing.push(format!("{file}: {name}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "components without a gallery row (add one to ui/gallery.slint):\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_component_card_points_at_an_existing_component() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/ux/components");
    let Ok(entries) = fs::read_dir(&dir) else {
        return; // no cards yet
    };
    let all: String = ["components.slint", "library.slint", "editor.slint"]
        .iter()
        .map(|f| ui(f))
        .collect();
    let re = regex::Regex::new(r"→ `([A-Za-z0-9_-]+)`").unwrap();
    let mut broken = Vec::new();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == "TEMPLATE.md" || path.extension().is_none_or(|e| e != "md")
        {
            continue;
        }
        let card = fs::read_to_string(&path).unwrap();
        for cap in re.captures_iter(&card) {
            let name = &cap[1];
            if name.starts_with("std:") {
                continue;
            }
            if !all.contains(&format!("component {name} ")) {
                broken.push(format!("{}: `{name}`", path.display()));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "spec cards naming unknown components:\n{}",
        broken.join("\n")
    );
}
