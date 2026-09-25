# Licensing notes

ClipForge itself has no licence yet (private use). This file collects what
matters the day it is distributed. Keep it current when dependencies change;
`cargo deny check` enforces the allow-list in `deny.toml`.

## Our own code

Decide on a licence before publishing (MIT/Apache-2.0 dual is the Rust
default; GPL-3.0 would be compatible with Slint's GPL option and FFmpeg's GPL
build). Add `license` to `[workspace.package]` and remove
`private = { ignore = true }` from `deny.toml` and `about.toml`.

## Slint (UI toolkit)

Triple-licensed: GPL-3.0-only, Slint Royalty-free Licence 2.0, Slint Software
Licence 3.0. We use the **Royalty-free Licence**, which permits proprietary
desktop applications free of charge with these obligations:

- Show "Made with Slint" attribution (About dialog and/or documentation).
- Do not use Slint for embedded devices under this licence.
- Full text: <https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md>

## Fluent UI System Icons

The UI icons in `assets/fluent/` are a subset of Microsoft's Fluent UI
System Icons, MIT licence (`assets/fluent/LICENSE.txt`). Attribution is
shown in Settings → About. Regenerate with `cargo xtask fluent-icons`.

## Inter (caption and title font)

Captions and title cards are drawn with Inter by Rasmus Andersson
(`assets/fonts/InterVariable.ttf`, v4.1), SIL Open Font License 1.1
(`assets/fonts/OFL.txt`). The font is embedded in the binary; the OFL
allows bundling it with software as long as the licence travels with it
and the font is not sold on its own.

## FFmpeg

- **Decoding** links `libav*` as shared or static libraries (from Milestone
  1/3 on). An LGPL-2.1+ build is enough for decoding; keep the libraries
  dynamically linked or provide relinkable objects to satisfy the LGPL.
- **Encoding** runs the `ffmpeg` executable as a separate process (sidecar).
  Bundling a GPL build (needed for `libx264`/`libx265` software encoders) is
  fine as long as it stays a separate executable and its source/licence is
  offered. Hardware encoders (VideoToolbox, Media Foundation, NVENC) work
  with an LGPL build.
- Record the exact build, its configure flags and its source URL in
  `docs/DEPENDENCIES.md` when the binary is added.

## Video codec patents

H.264 (AVC) and H.265 (HEVC) are patent-encumbered. Private use is not a
concern. For distribution beyond a hobby scale, prefer hardware encoders
(the OS vendor holds the licence) and consider AV1 as the software fallback.
On Windows, HEVC decoding needs Microsoft's "HEVC Video Extensions" from the
Store; the app should detect and explain this.

## Everything else

MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode-3.0, CC0, Unlicense, BSL-1.0
dependencies: attribution via the generated `THIRD_PARTY_LICENSES.html`,
which must ship with the app (About dialog link or installer folder).
MPL-2.0 (`option-ext`) requires that modifications to that crate, if any, be
published; we make none.

## Test fixtures and assets

`fixtures/` is generated from FFmpeg's synthetic sources (no third-party
content). `assets/icon.svg` and derived icons are original work.
