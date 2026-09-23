//! Generates small, licence-free test media so tests never depend on
//! personal photos. Everything is produced from ffmpeg's synthetic sources.
//!
//! `cargo xtask fixtures` regenerates all files and the manifest;
//! `cargo xtask fixtures --verify` only checks that every manifest entry
//! exists with the recorded size, which is what CI runs (no ffmpeg needed).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::icons::run_cmd;

pub(crate) const MANIFEST: &str = "fixtures/MANIFEST.txt";

/// Maximum total size we are willing to commit.
const MAX_TOTAL_BYTES: u64 = 200 * 1024;

struct Fixture {
    name: &'static str,
    description: &'static str,
    args: &'static [&'static str],
    /// If set, the generated file is remuxed once more with
    /// `-display_rotation <value>` so it carries a display matrix like
    /// phone footage does (lavfi inputs cannot get one directly).
    display_rotation: Option<&'static str>,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "photo_landscape.jpg",
        description: "320x180 SDR JPEG test pattern",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=1",
            "-frames:v",
            "1",
            "-q:v",
            "6",
        ],
        display_rotation: None,
    },
    Fixture {
        name: "photo_portrait.png",
        description: "90x160 SDR PNG gradient",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "gradients=size=90x160:rate=1",
            "-frames:v",
            "1",
        ],
        display_rotation: None,
    },
    Fixture {
        name: "photo_square.tif",
        description: "64x64 TIFF colour bars",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "smptebars=size=64x64:rate=1",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-compression_algo",
            "deflate",
        ],
        display_rotation: None,
    },
    Fixture {
        name: "video_sdr_h264.mp4",
        description: "2 s 320x180 25 fps H.264 8-bit BT.709 with 440 Hz tone",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "2",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "30",
            "-pix_fmt",
            "yuv420p",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
            "-c:a",
            "aac",
            "-b:a",
            "32k",
            "-movflags",
            "+faststart",
        ],
        display_rotation: None,
    },
    Fixture {
        name: "video_hlg_hevc.mp4",
        description: "2 s 320x180 25 fps HEVC 10-bit HLG BT.2020, no audio",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25",
            "-t",
            "2",
            "-c:v",
            "libx265",
            "-preset",
            "veryfast",
            "-crf",
            "32",
            "-pix_fmt",
            "yuv420p10le",
            "-x265-params",
            "log-level=error:colorprim=bt2020:transfer=arib-std-b67:colormatrix=bt2020nc",
            "-color_primaries",
            "bt2020",
            "-color_trc",
            "arib-std-b67",
            "-colorspace",
            "bt2020nc",
            "-an",
        ],
        display_rotation: None,
    },
    Fixture {
        name: "video_rotated_90.mov",
        description: "1 s H.264 stored 320x180 with a display matrix that shows it as 180x320 portrait",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25",
            "-t",
            "1",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "32",
            "-pix_fmt",
            "yuv420p",
            "-an",
        ],
        display_rotation: Some("-90"),
    },
    Fixture {
        name: "audio_stereo.m4a",
        description: "3 s stereo AAC, 220 Hz left / 330 Hz right",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:sample_rate=44100",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=330:sample_rate=44100",
            "-filter_complex",
            "[0:a][1:a]join=inputs=2:channel_layout=stereo[a]",
            "-map",
            "[a]",
            "-t",
            "3",
            "-c:a",
            "aac",
            "-b:a",
            "48k",
        ],
        display_rotation: None,
    },
    Fixture {
        name: "audio_mono.wav",
        description: "0.5 s mono 16-bit PCM 48 kHz",
        args: &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=48000",
            "-t",
            "0.5",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
        ],
        display_rotation: None,
    },
];

/// Hand-written fixtures that ffmpeg cannot produce.
const STATIC_FIXTURES: &[(&str, &str, &[u8])] = &[
    (
        ".photo_cloud_only.jpg.icloud",
        "macOS iCloud Drive placeholder stub (binary plist replaced by XML for readability)",
        br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>NSURLFileResourceTypeKey</key><string>NSURLFileResourceTypeRegular</string>
<key>NSURLFileSizeKey</key><integer>2451208</integer>
<key>NSURLNameKey</key><string>photo_cloud_only.jpg</string>
</dict></plist>
"#,
    ),
    ("broken_truncated.jpg", "JPEG header followed by garbage; decoders must fail cleanly", b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00garbage-not-a-jpeg"),
    ("empty.mp4", "zero-byte file with a video extension", b""),
];

pub(crate) fn run(args: &[String]) -> Result<()> {
    let root = workspace_root()?;
    let dir = root.join("fixtures");
    if args.iter().any(|a| a == "--verify") {
        return verify(&root);
    }
    std::fs::create_dir_all(&dir)?;
    let mut manifest =
        String::from("# Generated by `cargo xtask fixtures`. name<TAB>bytes<TAB>description\n");
    for f in FIXTURES {
        let out = dir.join(f.name);
        let first_pass_out = match f.display_rotation {
            None => out.clone(),
            Some(_) => dir.join(format!("tmp-{}", f.name)),
        };
        run_cmd(
            Command::new("ffmpeg")
                .args(["-y", "-loglevel", "error"])
                .args(f.args)
                .arg(&first_pass_out),
        )
        .with_context(|| format!("generating {}", f.name))?;
        if let Some(rotation) = f.display_rotation {
            // lavfi inputs cannot carry a display matrix; remux to add one.
            run_cmd(
                Command::new("ffmpeg")
                    .args([
                        "-y",
                        "-loglevel",
                        "error",
                        "-display_rotation",
                        rotation,
                        "-i",
                    ])
                    .arg(&first_pass_out)
                    .args(["-c", "copy"])
                    .arg(&out),
            )
            .with_context(|| format!("adding display matrix to {}", f.name))?;
            std::fs::remove_file(&first_pass_out)?;
        }
        let size = std::fs::metadata(&out)?.len();
        manifest.push_str(&format!("{}\t{}\t{}\n", f.name, size, f.description));
    }
    for (name, description, bytes) in STATIC_FIXTURES {
        std::fs::write(dir.join(name), bytes)?;
        manifest.push_str(&format!("{}\t{}\t{}\n", name, bytes.len(), description));
    }
    std::fs::write(root.join(MANIFEST), &manifest)?;
    let total = total_size(&manifest)?;
    println!(
        "wrote {} fixtures, {} bytes total",
        FIXTURES.len() + STATIC_FIXTURES.len(),
        total
    );
    if total > MAX_TOTAL_BYTES {
        bail!("fixtures exceed {MAX_TOTAL_BYTES} bytes; shrink them before committing");
    }
    Ok(())
}

fn verify(root: &Path) -> Result<()> {
    let manifest =
        std::fs::read_to_string(root.join(MANIFEST)).context("reading fixtures manifest")?;
    let mut problems = Vec::new();
    for (name, size) in parse_manifest(&manifest) {
        match std::fs::metadata(root.join("fixtures").join(name)) {
            Ok(m) if m.len() == size => {}
            Ok(m) => problems.push(format!("{name}: expected {size} bytes, found {}", m.len())),
            Err(e) => problems.push(format!("{name}: {e}")),
        }
    }
    if problems.is_empty() {
        println!("fixtures OK");
        Ok(())
    } else {
        for p in &problems {
            eprintln!("{p}");
        }
        bail!(
            "{} fixture problem(s); run `cargo xtask fixtures`",
            problems.len()
        );
    }
}

fn parse_manifest(manifest: &str) -> impl Iterator<Item = (&str, u64)> {
    manifest
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let name = parts.next()?;
            let size = parts.next()?.parse().ok()?;
            Some((name, size))
        })
}

fn total_size(manifest: &str) -> Result<u64> {
    Ok(parse_manifest(manifest).map(|(_, s)| s).sum())
}

pub(crate) fn workspace_root() -> Result<PathBuf> {
    let meta = cargo_metadata::MetadataCommand::new().no_deps().exec()?;
    Ok(meta.workspace_root.into_std_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_and_skips_comments() {
        let m = "# c\na.jpg\t10\tx\n\nb.mp4\t20\ty\n";
        let v: Vec<_> = parse_manifest(m).collect();
        assert_eq!(v, [("a.jpg", 10), ("b.mp4", 20)]);
        assert_eq!(total_size(m).unwrap(), 30);
    }

    #[test]
    fn committed_fixtures_match_manifest() {
        let root = workspace_root().unwrap();
        if root.join(MANIFEST).exists() {
            verify(&root).unwrap();
        }
    }

    #[test]
    fn fixture_names_are_unique() {
        let mut names: Vec<&str> = FIXTURES
            .iter()
            .map(|f| f.name)
            .chain(STATIC_FIXTURES.iter().map(|s| s.0))
            .collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(n, names.len());
    }
}
