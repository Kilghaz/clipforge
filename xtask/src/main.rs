//! Developer tasks. Run with `cargo xtask <command>`.

#![forbid(unsafe_code)]
#![allow(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod deps;
mod fixtures;
mod fluent_icons;
mod icons;

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    match cmd.as_str() {
        "check-deps" => deps::check(),
        "fixtures" => fixtures::run(&rest),
        "icons" => icons::run(),
        "fluent-icons" => fluent_icons::run(&rest),
        _ => {
            eprintln!(
                "usage: cargo xtask <command>\n\n  check-deps        verify the crate dependency direction from docs/PLAN.md\n  fixtures [--verify] generate test media into fixtures/ (needs ffmpeg), or verify the manifest\n  icons             regenerate assets/icons from assets/icon.svg (needs rsvg-convert or ffmpeg, iconutil on macOS)\n  fluent-icons [--verify] fetch the UI icon subset into assets/fluent (needs curl), or verify it is complete"
            );
            bail!("unknown command {cmd:?}");
        }
    }
}
