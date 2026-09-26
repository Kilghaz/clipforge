//! `cargo xtask ffmpeg-bundle`: fetches the ffmpeg build the Windows
//! installer ships (ADR-0012) into `target/ffmpeg-bundle/`, verified
//! against a pinned SHA-256. cargo-packager picks the folder up as the
//! `ffmpeg` resource next to `clipforge.exe`; without it the installer is
//! built without ffmpeg (a glob that matches nothing).
//!
//! Uses the OS tools only: `curl`, `tar` (reads zip on Windows 10+ and
//! macOS) and `certutil` / `shasum` / `sha256sum` for the hash.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// gyan.dev "essentials" release build: libx264, libx265, NVENC, AMF, QSV
/// (libvpl), Media Foundation, D3D11VA. GPL-3.0.
pub(crate) const VERSION: &str = "9.0.2";
const URL: &str = "https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.zip";
const SHA256: &str = "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba";
const TOP: &str = "ffmpeg-9.0.2-essentials_build";
/// Archive members that go into the bundle, and their names there.
const MEMBERS: &[(&str, &str)] = &[
    ("bin/ffmpeg.exe", "ffmpeg.exe"),
    ("bin/ffprobe.exe", "ffprobe.exe"),
    ("LICENSE", "LICENSE.txt"),
    ("README.txt", "README.txt"),
];

/// Written next to the binaries: where the corresponding source is (GPL).
fn source_note() -> String {
    format!(
        "This folder contains FFmpeg {VERSION} (ffmpeg.exe, ffprobe.exe) as built by\n\
         gyan.dev (https://www.gyan.dev/ffmpeg/builds/), licensed under the GNU GPL\n\
         version 3 (LICENSE.txt). ClipForge runs these programs as separate processes.\n\n\
         Corresponding source code:\n\
         - FFmpeg: https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz\n\
         - Build configuration and the libraries' sources: see README.txt and\n\
           https://www.gyan.dev/ffmpeg/builds/\n\
         Download used: {URL}\nSHA-256: {SHA256}\n"
    )
}

pub(crate) fn run(_args: &[String]) -> Result<()> {
    let root = crate::fixtures::workspace_root()?;
    let download_dir = root.join("target/ffmpeg-download");
    let out = root.join("target/ffmpeg-bundle");
    std::fs::create_dir_all(&download_dir)?;
    let archive = download_dir.join(format!("{TOP}.zip"));
    if !archive.is_file() || sha256(&archive)? != SHA256 {
        println!("downloading {URL}");
        let status = Command::new("curl")
            .args(["-sSfL", "--retry", "3", "-o"])
            .arg(&archive)
            .arg(URL)
            .status()
            .context("running curl")?;
        if !status.success() {
            bail!("download failed: {status}");
        }
    }
    let got = sha256(&archive)?;
    if got != SHA256 {
        bail!(
            "checksum mismatch for {}: {got}, expected {SHA256}",
            archive.display()
        );
    }
    let unpack = download_dir.join("unpacked");
    let _ = std::fs::remove_dir_all(&unpack);
    std::fs::create_dir_all(&unpack)?;
    let mut tar = Command::new("tar");
    tar.arg("-xf").arg(&archive).arg("-C").arg(&unpack);
    for (member, _) in MEMBERS {
        tar.arg(format!("{TOP}/{member}"));
    }
    let status = tar.status().context("running tar")?;
    if !status.success() {
        bail!("extracting failed: {status}");
    }
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    for (member, name) in MEMBERS {
        let from = unpack.join(TOP).join(member);
        std::fs::copy(&from, out.join(name))
            .with_context(|| format!("copying {}", from.display()))?;
    }
    std::fs::write(out.join("SOURCE.txt"), source_note())?;
    println!("ffmpeg {VERSION} bundle ready in {}", out.display());
    if !cfg!(windows) {
        println!(
            "note: these are Windows binaries; every package built while the folder exists includes them. Delete {} before packaging for macOS.",
            out.display()
        );
    }
    Ok(())
}

/// Lower-case hex SHA-256 of `path`, via the OS tool.
fn sha256(path: &Path) -> Result<String> {
    let path: PathBuf = path.to_path_buf();
    let output = if cfg!(windows) {
        Command::new("certutil")
            .arg("-hashfile")
            .arg(&path)
            .arg("SHA256")
            .output()
    } else if Command::new("shasum").arg("--version").output().is_ok() {
        Command::new("shasum")
            .args(["-a", "256"])
            .arg(&path)
            .output()
    } else {
        Command::new("sha256sum").arg(&path).output()
    }
    .context("running the hash tool")?;
    if !output.status.success() {
        bail!("hashing {} failed", path.display());
    }
    parse_hash(&String::from_utf8_lossy(&output.stdout))
        .with_context(|| format!("no hash in the tool output for {}", path.display()))
}

/// Finds the 64-digit hex hash in `shasum`, `sha256sum` or `certutil`
/// output (certutil prints it on its own line, older versions with spaces).
fn parse_hash(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let first = line.split_whitespace().next()?;
        let candidate: String = if first.len() == 64 {
            first.to_owned()
        } else {
            line.chars().filter(|c| !c.is_whitespace()).collect()
        };
        (candidate.len() == 64 && candidate.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| candidate.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba";

    #[test]
    fn reads_hashes_from_every_tool() {
        assert_eq!(parse_hash(&format!("{H}  ff.zip\n")).as_deref(), Some(H));
        let certutil = format!(
            "SHA256 hash of ff.zip:\r\n{}\r\nCertUtil: -hashfile command completed successfully.\r\n",
            H.to_uppercase()
        );
        assert_eq!(parse_hash(&certutil).as_deref(), Some(H));
        let spaced: String = H
            .as_bytes()
            .chunks(2)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            parse_hash(&format!("header\n{spaced}\n")).as_deref(),
            Some(H)
        );
        assert_eq!(parse_hash("CertUtil: error"), None);
    }

    #[test]
    fn source_note_names_version_and_checksum() {
        let note = source_note();
        assert!(note.contains(VERSION) && note.contains(SHA256) && note.contains("GPL"));
    }
}
