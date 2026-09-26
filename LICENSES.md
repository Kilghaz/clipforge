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

## Text fonts

Texts are drawn with six fonts embedded in the binary, all under the SIL
Open Font License 1.1 (licence texts next to the files in `assets/fonts/`):
Inter by Rasmus Andersson (`InterVariable.ttf`, v4.1, `OFL-inter.txt`),
Montserrat, Playfair Display, Bebas Neue, Dancing Script and Caveat (from
the Google Fonts repository, `OFL-<name>.txt`). The OFL allows bundling
them with software as long as the licence travels with them and the fonts
are not sold on their own. Texts may also use fonts installed on the
user's machine; those are not shipped, and their licences (e.g. for
embedding in rendered video) are the user's matter.

## FFmpeg

- **Decoding and encoding** both run the `ffmpeg` / `ffprobe` executables as
  separate processes (ADR-0003, ADR-0008); nothing links `libav*`.
- **Windows installer** (ADR-0012): ships FFmpeg 9.0.2 as built by gyan.dev
  ("essentials", GPL-3.0, includes libx264/libx265 and the NVENC, AMF, QSV
  and Media Foundation encoders) in `ffmpeg\` next to `clipforge.exe`,
  with its `LICENSE.txt`, the build's `README.txt` (configure flags and
  library versions) and `SOURCE.txt` (source URLs, download URL, SHA-256).
  As separate programs they do not affect ClipForge's own licence. The
  download is pinned and verified in `xtask/src/ffmpeg_bundle.rs`.
- **macOS**: ffmpeg is not bundled yet; the app uses one on `PATH`
  (Homebrew) or `CLIPFORGE_FFMPEG_DIR`.

## Video codec patents

H.264 (AVC) and H.265 (HEVC) are patent-encumbered. Private use is not a
concern. For distribution beyond a hobby scale, prefer hardware encoders
(the OS vendor holds the licence) and consider AV1 as the software fallback.
On Windows, playing HEVC files in Photos / Media Player needs Microsoft's
"HEVC Video Extensions" from the Store; the export window detects a missing
extension and links to the free edition (ClipForge's own decoding uses the
bundled ffmpeg and does not need it).

## Everything else

MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode-3.0, CC0, Unlicense, BSL-1.0
dependencies: attribution via the generated `THIRD_PARTY_LICENSES.html`,
which must ship with the app (About dialog link or installer folder).
MPL-2.0 (`option-ext`) requires that modifications to that crate, if any, be
published; we make none.

## Test fixtures and assets

`fixtures/` is generated from FFmpeg's synthetic sources (no third-party
content). `assets/icon.svg` and derived icons are original work.
