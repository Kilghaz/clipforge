# ADR-0012: The Windows installer bundles a pinned gyan.dev ffmpeg build

Date: 2026-09-26
Status: Accepted

## Context

ClipForge decodes and encodes through the `ffmpeg` / `ffprobe` executables
(ADR-0003, ADR-0008). Windows has no system ffmpeg, so an installer without
one cannot read videos or HEIC photos or export anything. M7 asks for
hardware encoders (NVENC, QSV, AMF, Media Foundation) and a real installer.

## Decision

- The NSIS installer ships `ffmpeg.exe` and `ffprobe.exe` in an `ffmpeg\`
  folder next to `clipforge.exe`, where `FfmpegLocation::discover` already
  looks.
- The build is gyan.dev's "essentials" release (9.0.2, GPL-3.0) from its
  GitHub releases (stable, versioned URLs): it contains libx264/libx265,
  NVENC, AMF, QSV (libvpl), Media Foundation and D3D11VA. Static
  executables, no DLLs to manage.
- `cargo xtask ffmpeg-bundle` downloads it to `target/ffmpeg-bundle/`,
  verifies a pinned SHA-256 and adds the licence, the build readme and a
  source note; cargo-packager includes the folder as a resource. CI runs it
  before packaging and smoke-tests the silent install.
- Which encoder runs is decided at export time by a test encode per
  hardware encoder (listed is not present), with software fallback.
- Rejected: BtbN builds (their release tags are pruned, pinned URLs break),
  shared builds (more files, no size win after compression), an LGPL build
  (no libx264/libx265 fallback on machines without a usable GPU encoder),
  downloading ffmpeg on first start (network dependency, worse privacy).

## Consequences

- The installer grows by roughly 70 MB compressed.
- Updating ffmpeg means changing the version, URL and hash in
  `xtask/src/ffmpeg_bundle.rs` and checking the e2e export suite.
- macOS still relies on a system ffmpeg; bundling one there is open.
