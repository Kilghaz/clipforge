//! Regenerates the application icons from `assets/icon.svg`.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const SIZES: &[u32] = &[16, 32, 64, 128, 256, 512, 1024];

pub(crate) fn run() -> Result<()> {
    let root = crate::fixtures::workspace_root()?;
    let svg = root.join("assets/icon.svg");
    let out = root.join("assets/icons");
    std::fs::create_dir_all(&out)?;

    for size in SIZES {
        let png = out.join(format!("{size}x{size}.png"));
        rasterize(&svg, &png, *size)?;
    }
    std::fs::copy(out.join("512x512.png"), out.join("icon.png"))?;

    // Windows .ico: ffmpeg's ico muxer takes a single PNG frame; 256 px is the
    // largest size Explorer uses.
    let ico = out.join("icon.ico");
    run_cmd(
        Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-i"])
            .arg(out.join("256x256.png"))
            .arg(&ico),
    )?;

    if cfg!(target_os = "macos") {
        let iconset = out.join("icon.iconset");
        std::fs::create_dir_all(&iconset)?;
        for (name, size) in [
            ("icon_16x16.png", 16),
            ("icon_16x16@2x.png", 32),
            ("icon_32x32.png", 32),
            ("icon_32x32@2x.png", 64),
            ("icon_128x128.png", 128),
            ("icon_128x128@2x.png", 256),
            ("icon_256x256.png", 256),
            ("icon_256x256@2x.png", 512),
            ("icon_512x512.png", 512),
            ("icon_512x512@2x.png", 1024),
        ] {
            std::fs::copy(out.join(format!("{size}x{size}.png")), iconset.join(name))?;
        }
        run_cmd(
            Command::new("iconutil")
                .args(["-c", "icns", "-o"])
                .arg(out.join("icon.icns"))
                .arg(&iconset),
        )?;
        std::fs::remove_dir_all(&iconset)?;
    } else {
        eprintln!("skipping icon.icns (needs macOS iconutil)");
    }
    println!("icons written to {}", out.display());
    Ok(())
}

fn rasterize(svg: &Path, png: &Path, size: u32) -> Result<()> {
    if Command::new("rsvg-convert")
        .arg("--version")
        .output()
        .is_ok()
    {
        return run_cmd(
            Command::new("rsvg-convert")
                .args(["-w", &size.to_string(), "-h", &size.to_string(), "-o"])
                .arg(png)
                .arg(svg),
        );
    }
    if cfg!(target_os = "macos") {
        // qlmanage renders SVG via Quick Look; output name is <file>.png.
        let tmp = tempfile_dir()?;
        run_cmd(
            Command::new("qlmanage")
                .args(["-t", "-s", &size.to_string(), "-o"])
                .arg(&tmp)
                .arg(svg),
        )?;
        let produced = tmp.join(format!(
            "{}.png",
            svg.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("icon.svg")
        ));
        // Quick Look may pad; normalise with ffmpeg scale.
        run_cmd(
            Command::new("ffmpeg")
                .args(["-y", "-loglevel", "error", "-i"])
                .arg(&produced)
                .args(["-vf", &format!("scale={size}:{size}")])
                .arg(png),
        )?;
        return Ok(());
    }
    bail!("need rsvg-convert (librsvg) to rasterize {}", svg.display());
}

fn tempfile_dir() -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("clipforge-icons-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn run_cmd(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to start {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("{:?} exited with {status}", cmd.get_program());
    }
    Ok(())
}
