//! Fetches the curated subset of Microsoft's Fluent UI System Icons (MIT)
//! that the app uses into `assets/fluent/`.
//!
//! Icons are single-colour SVGs; the UI recolours them with `colorize`, so
//! only the regular weight at 20 px is needed (plus a few filled variants for
//! "on" states). Keep this list minimal: every entry is ~1 KB in the repo.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const REPO_RAW: &str = "https://raw.githubusercontent.com/microsoft/fluentui-system-icons/main";

/// (folder name in the upstream repo, snake_case icon name, filled?)
const ICONS: &[(&str, &str, bool)] = &[
    ("Add", "add", false),
    ("Arrow Redo", "arrow_redo", false),
    ("Arrow Rotate Clockwise", "arrow_rotate_clockwise", false),
    ("Arrow Sort Down", "arrow_sort_down", false),
    ("Arrow Sort Up", "arrow_sort_up", false),
    ("Arrow Undo", "arrow_undo", false),
    ("Checkmark Circle", "checkmark_circle", false),
    ("Chevron Down", "chevron_down", false),
    ("Chevron Up", "chevron_up", false),
    ("Cloud", "cloud", false),
    ("Delete", "delete", false),
    ("Dismiss", "dismiss", false),
    ("Document Add", "document_add", false),
    ("Error Circle", "error_circle", false),
    ("Filmstrip", "filmstrip", false),
    ("Folder Add", "folder_add", false),
    ("Folder Open", "folder_open", false),
    ("Image", "image", false),
    ("Image Multiple", "image_multiple", false),
    ("Info", "info", false),
    ("Music Note 2", "music_note_2", false),
    ("Panel Left Contract", "panel_left_contract", false),
    ("Panel Left Expand", "panel_left_expand", false),
    ("Pause", "pause", true),
    ("Play", "play", true),
    ("Save", "save", false),
    ("Search", "search", false),
    ("Select All On", "select_all_on", false),
    ("Settings", "settings", false),
    ("Share", "share", false),
    ("Speaker 2", "speaker_2", false),
    ("Speaker Mute", "speaker_mute", false),
    ("Video", "video", false),
    ("Warning", "warning", false),
    ("Zoom In", "zoom_in", false),
    ("Zoom Out", "zoom_out", false),
];

const LICENSE: &str = "Fluent UI System Icons
Copyright (c) 2020 Microsoft Corporation

MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

Source: https://github.com/microsoft/fluentui-system-icons
";

pub(crate) fn run(args: &[String]) -> Result<()> {
    let root = crate::fixtures::workspace_root()?;
    let out = root.join("assets/fluent");
    if args.iter().any(|a| a == "--verify") {
        return verify(&out);
    }
    std::fs::create_dir_all(&out)?;
    for (folder, name, filled) in ICONS {
        let url = icon_url(folder, name, *filled);
        let target = out.join(format!("{name}.svg"));
        let output = Command::new("curl")
            .args(["-sSfL", "--max-time", "30", &url])
            .output()
            .context("running curl")?;
        if !output.status.success() {
            bail!(
                "download failed for {url}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let svg = String::from_utf8(output.stdout).context("icon is not UTF-8")?;
        if !svg.contains("<svg") {
            bail!("{url} did not return an SVG");
        }
        std::fs::write(&target, svg)?;
    }
    std::fs::write(out.join("LICENSE.txt"), LICENSE)?;
    println!("{} icons written to {}", ICONS.len(), out.display());
    Ok(())
}

fn icon_url(folder: &str, name: &str, filled: bool) -> String {
    let weight = if filled { "filled" } else { "regular" };
    let folder = folder.replace(' ', "%20");
    format!("{REPO_RAW}/assets/{folder}/SVG/ic_fluent_{name}_20_{weight}.svg")
}

/// Checks that every icon in the list exists on disk (no network).
fn verify(out: &Path) -> Result<()> {
    let mut missing = Vec::new();
    for (_, name, _) in ICONS {
        if !out.join(format!("{name}.svg")).is_file() {
            missing.push(*name);
        }
    }
    if !out.join("LICENSE.txt").is_file() {
        missing.push("LICENSE.txt");
    }
    if !missing.is_empty() {
        bail!("missing in {}: {}", out.display(), missing.join(", "));
    }
    println!("{} icons present", ICONS.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_the_upstream_layout() {
        assert_eq!(
            icon_url("Folder Add", "folder_add", false),
            "https://raw.githubusercontent.com/microsoft/fluentui-system-icons/main/assets/Folder%20Add/SVG/ic_fluent_folder_add_20_regular.svg"
        );
        assert!(icon_url("Play", "play", true).ends_with("_20_filled.svg"));
    }

    #[test]
    fn icon_names_are_unique_and_snake_case() {
        let mut names: Vec<&str> = ICONS.iter().map(|(_, n, _)| *n).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate icon names");
        for n in names {
            assert!(
                n.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{n}"
            );
        }
    }
}
