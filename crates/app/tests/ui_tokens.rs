//! Design-token lint for the Slint UI (docs/ux/DESIGN.md §2): colours, radii
//! and font sizes are defined once in `ui/theme.slint`; every other `.slint`
//! file must reference `Theme.*` / `Palette.*` instead of literals.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

fn ui_files() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("ui directory") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "slint")
            && path.file_name().is_some_and(|n| n != "theme.slint")
        {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, fs::read_to_string(&path).expect("read slint")));
        }
    }
    assert!(out.len() >= 5, "expected the UI files, found {}", out.len());
    out
}

/// Lines to check: code only, no comments.
fn code_lines(src: &str) -> impl Iterator<Item = (usize, &str)> {
    src.lines().enumerate().filter_map(|(i, l)| {
        let t = l.trim_start();
        (!t.starts_with("//")).then_some((i + 1, l))
    })
}

fn find(pattern: &str, describe: &str) -> Vec<String> {
    let re = regex::Regex::new(pattern).expect("valid regex");
    let mut hits = Vec::new();
    for (name, src) in ui_files() {
        for (line, text) in code_lines(&src) {
            if re.is_match(text) {
                hits.push(format!("{name}:{line}: {describe}: {}", text.trim()));
            }
        }
    }
    hits
}

#[test]
fn no_hex_colours_outside_theme() {
    let hits = find(r"#[0-9a-fA-F]{3,8}\b", "hex colour");
    assert!(
        hits.is_empty(),
        "use Theme/Palette tokens:\n{}",
        hits.join("\n")
    );
}

#[test]
fn no_named_colours_outside_theme() {
    let hits = find(
        r"(color|background|border-color):\s*(white|black|red|green|blue|gray|grey)\s*;",
        "named colour",
    );
    assert!(
        hits.is_empty(),
        "use Theme/Palette tokens:\n{}",
        hits.join("\n")
    );
}

#[test]
fn radii_and_font_sizes_come_from_theme() {
    let hits = find(
        r"(border-radius|font-size):\s*[0-9]+(\.[0-9]+)?px\s*;",
        "literal size",
    );
    assert!(
        hits.is_empty(),
        "use Theme.radius-* / Theme.font-*:\n{}",
        hits.join("\n")
    );
}

#[test]
fn icon_only_buttons_use_icon_button() {
    // A std Button with an icon and no text cannot centre the icon.
    let hits = find(
        r"\bButton\s*\{[^}]*\bicon:[^}]*\}",
        "std Button with icon; use IconButton or ActionButton",
    );
    let bad: Vec<_> = hits
        .into_iter()
        .filter(|h| !h.contains("text:") && !h.contains("gallery.slint"))
        .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
