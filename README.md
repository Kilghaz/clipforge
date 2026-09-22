# ClipForge

A fast, native desktop app for macOS and Windows that turns hundreds or
thousands of photos and videos into a slideshow video. Drop files in, bulk
apply durations and transitions, add music, export in Full HD or 4K, with
optional HDR and a YouTube preset.

Built in Rust with [Slint](https://slint.dev) for the UI, wgpu for rendering,
SQLite for the media catalogue and FFmpeg for decoding and encoding. No web
technology.

**Status:** Milestone 0 (foundation). See `docs/PLAN.md` for the roadmap.

## Building

```sh
rustup show                 # picks up rust-toolchain.toml
cargo run -p clipforge-app  # opens the (still empty) app
cargo nextest run --workspace
```

Developer tooling and the full check pipeline are described in `AGENTS.md`.
Packaging (`.app`/`.dmg` on macOS, NSIS installer on Windows):

```sh
cargo install cargo-packager --locked
cargo build --release -p clipforge-app && cargo packager --release -p clipforge-app
```

## Licensing

ClipForge is currently a private project without a published licence. If it
is ever distributed, read `LICENSES.md` first: it lists the obligations that
come with Slint, FFmpeg and the video codecs, and what has to happen before a
release. Third-party licence texts are generated with
`cargo about generate about.hbs -o target/THIRD_PARTY_LICENSES.html`.
