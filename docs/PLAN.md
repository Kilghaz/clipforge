# ClipForge – Implementation Plan

Status: v1 (2026-09-22). Milestones 0 and 1 implemented; see §10 for deviations. Refinement pass with fresh context still to do.

ClipForge is a native desktop app for macOS and Windows that turns hundreds or
thousands of photos and videos into a slideshow video. Think Clipchamp, minus
the fluff, plus first-class bulk editing, a fast local media library, modern
formats (HEIC, HEVC, HDR) and export options a non-technical person understands.

## 1. Product

### 1.1 Goals (in priority order)

1. **Usability first.** A non-technical user must be able to drop in a folder
   of photos, pick a transition, pick music, press export, and get a good
   video. Every screen should be explainable in one sentence.
2. **Fast and stable.** No crashes, no beach balls. Long operations run in the
   background, are cancellable and report progress. Auto-save means a crash
   never loses more than a few seconds of work.
3. **Scale.** Thousands of media items in the library and hundreds of clips on
   the timeline without noticeable slowdown.

### 1.2 Non-goals for v1

Multi-track video compositing, keyframe animation editor, filters and colour
grading, screen or webcam recording, stock media, cloud APIs, any AI features,
collaboration, mobile.

### 1.3 Feature set v1

**Media library**
- Drop files and folders from anywhere: Finder, Explorer, Apple Photos, Windows
  Photos, Lightroom exports. Files stay where they are; the library only links
  to them.
- Supported inputs: JPEG, PNG, WebP, HEIC/HEIF (incl. Apple gain-map HDR), TIFF,
  common camera RAW (via embedded preview first, full demosaic later), GIF
  (first frame); MP4/MOV/M4V/MKV with H.264, HEVC (incl. HLG/Dolby Vision
  profile 8.4 iPhone footage), AV1, ProRes; audio MP3, AAC, M4A, WAV, FLAC.
- Grid view with instant thumbnails, virtualised so 10k items scroll smoothly.
  Sort by capture date, name, type; filter by type, date range, "used in
  project"; quick text search.
- Cloud placeholder awareness: files that live in iCloud Drive / iCloud for
  Windows / OneDrive but are not downloaded are shown using the OS thumbnail
  and marked with a cloud badge. Downloading is triggered only for a
  high-resolution preview or for export, with an explicit progress indicator.
- Relink: if a file was moved, find it again by content fingerprint.

**Timeline**
- One video track: an ordered sequence of clips (photos and videos) with a
  transition slot between neighbours. Two audio tracks for music/narration.
- Per clip: duration (photos), trim in/out (videos), fit/crop/rotate, Ken
  Burns preset (none, slow zoom in/out, pan left/right, random), mute video
  audio, volume.
- Transitions from a fixed library (cut, cross dissolve, fade through black or
  white, slide, wipe, zoom). Per-transition duration.
- Titles: opening title, closing card, per-clip caption with a handful of
  styles. Captions can be auto-filled from file date or file name.
- Music: add tracks, auto-fit to slideshow length (trim + fade out), fade
  in/out, loop, volume, duck under video audio.
- **Bulk edits on any selection** (and "select all" is one click): set
  duration, set/randomise transition, set transition duration, set Ken Burns,
  set fit mode, set caption style, remove captions, reverse order, sort by
  date, "fit slideshow to music length" (distributes photo durations).
- Undo/redo for everything, unlimited within a session.
- Aspect ratio 16:9 or 9:16 per project. Project frame rate 30 fps default
  (25/24/60 selectable in advanced settings).

**Preview**
- Realtime playback at reduced resolution (540p/720p) with scrubbing,
  loop-selection, and JKL keys. Video sources use hardware decoding of the
  original or a background-generated proxy, whichever is available.
- Preview quality never blocks editing: while a proxy or thumbnail is missing
  the frame shows a placeholder and updates when ready.

**Export**
- Dialog with: Resolution (1080p / 4K), Quality (Good / Better / Best),
  "HDR (for iPhone and YouTube HDR)" checkbox, off by default and only enabled
  when at least one HDR source is on the timeline, "Optimise for YouTube"
  checkbox, destination.
- Under an "Advanced" disclosure: codec (H.264 / HEVC), frame rate, bitrate,
  audio bitrate, container.
- Background export with progress, ETA, cancel, and a notification when done.
  The app remains usable during export.

**Housekeeping**
- English and German UI, switchable at runtime, system language by default.
- Auto-save, crash recovery, recent projects.
- Project file is a single JSON document; media is referenced, never copied.

## 2. Technology decisions

| Area | Decision | Rationale |
|---|---|---|
| Language | Rust, stable toolchain, edition 2024, MSRV = current stable at kickoff | Memory safety, performance, single language across all crates, cross-platform. |
| UI | Slint 1.18 (winit backend, femtovg renderer; `renderer-femtovg-wgpu` planned for the preview texture), `cupertino` style on macOS, `fluent` on Windows | Declarative, GPU rendered, real text input and accessibility, royalty-free desktop licence, wgpu texture import for the preview. |
| GPU compositing | wgpu (Metal / DX12 / Vulkan) | One shader codebase for preview and export; runs headless in tests. |
| Decoding | `ffmpeg-next` (libav* 7.x) in-process with hwaccel (VideoToolbox, D3D11VA); ImageIO via `objc2` on macOS for HEIC and gain-map HDR; `image` crate for PNG/JPEG/WebP fast paths | Broadest format coverage; Apple is the only correct decoder for Apple HDR photos. |
| Encoding | ffmpeg CLI as sidecar process, fed raw frames over a pipe, using hardware encoders (`h264_videotoolbox`, `hevc_videotoolbox`, `h264_mf`/`hevc_mf`, NVENC/QSV/AMF when present) with libx264/libx265 fallback | Process isolation for the longest-running operation, no native Media Foundation code, correct HDR tagging, GPL binary stays separate from the app. |
| Media library DB | SQLite via `rusqlite` (bundled), WAL mode, FTS5 for search; thumbnails and proxies as files in a content-addressed cache directory | Same choice as Lightroom, Apple Photos and digiKam. Single file, zero admin, handles millions of rows, trivially testable in memory. A dedicated "multimedia database" would add operational cost without a measurable benefit at 10k–100k items; the speed comes from the schema, indexes and the proxy cache, not from the engine. |
| Image processing | `image`, `fast_image_resize` (SIMD), `rawler` for camera RAW | Fast thumbnail generation on CPU thread pool. |
| Async / threads | `tokio` for IO and job orchestration, `rayon` for CPU pools, dedicated threads for decode | Clear separation of IO, CPU and GPU work. |
| i18n | Slint `@tr()` + `gettext` `.po` files, `fluent` not needed | Native Slint support, translators can use standard tools. |
| Serialisation | `serde` + JSON for project files with explicit `version` field and migrations | Human-readable, diffable, easy to test. |
| Time | Integer ticks in **flicks** (1/705 600 000 s) | Divides evenly into 24/25/30/48/50/60 fps and 44.1/48/96 kHz, no drift, no floats in the model. |
| Errors | `thiserror` in libraries, `anyhow` only at the app edge, `tracing` for logs | Typed errors are testable. |
| Testing | `cargo test`, `proptest`, `insta` snapshots, golden-image tests with tolerance, `criterion` benches | See section 5. |
| Packaging | `cargo-bundle`/`cargo-packager` producing `.app` + `.dmg` and `.msi`; ffmpeg binary bundled | Double-click install on both platforms. |
| CI | GitHub Actions, matrix macos-14 (arm64) + windows-latest | Windows never rots. |

Rejected: web technology of any kind (Tauri, Electron, Dioxus web), SwiftUI +
WinUI dual UI, native Media Foundation encoding, in-process encoding, floats
for time, storing thumbnails as SQLite blobs (write amplification, WAL bloat).

### 2.1 Licensing notes (for `LICENSES.md` and `README.md`)

- Slint: Royalty-Free Licence for desktop applications; must be declared.
- FFmpeg: bundled as a separate GPL binary (sidecar) and linked as LGPL
  libraries for decoding. A GPL build of ffmpeg enables libx264/libx265;
  distributing it means the sidecar is GPL, which is fine as long as it stays a
  separate executable. Document build flags.
- HEVC/H.264 patents: private use is fine; distribution beyond a hobby scale
  may need attention. Hardware encoders shift that to the OS vendor.
- `rawler`, `image`, `wgpu`, `tokio`, `rusqlite`: MIT/Apache-2.0.
- `cargo-about` generates the third-party licence report in CI.

## 3. Architecture

### 3.1 Principles

1. **The core knows nothing about the UI or the OS.** `clipforge-core` and
   `clipforge-render` compile on any platform without Slint, Cocoa or Win32.
2. **Everything the user does is a `Command`.** Commands are data, applied to an
   immutable-style `Project`, produce an inverse command, and are the only way
   state changes. Bulk edits are single commands (one undo step).
3. **Rendering is a pure function** of `(Project, time, RenderQuality) ->
   Frame`. Preview and export call the same function with different quality.
4. **All slow work is a Job.** Jobs are cancellable, report progress, run off
   the UI thread, and are prioritised (visible thumbnails before invisible).
5. **Crashes in unsafe territory are contained.** Encoding runs out of process;
   decoding is wrapped with panic boundaries and a watchdog; a decoder that
   fails on a file marks the file as broken instead of taking the app down.

### 3.2 Crate layout (Cargo workspace)

```
clipforge/
├── Cargo.toml                 workspace
├── AGENTS.md                  agent rules (CLAUDE.md symlinks to it)
├── README.md, LICENSES.md
├── docs/
│   ├── PLAN.md                this file
│   ├── ARCHITECTURE.md        living document, redrawn from code
│   ├── adr/                   architecture decision records
│   └── ux/                    screen descriptions, keyboard map
├── crates/
│   ├── core/       (clipforge-core)      domain model, commands, undo, bulk ops, timeline maths, export presets. No IO, no async.
│   ├── media/      (clipforge-media)     Decoder/Prober traits + backends (ffmpeg, apple-imageio, image). Frame types, colour metadata.
│   ├── library/    (clipforge-library)   SQLite media library, import pipeline, fingerprinting, thumbnail/proxy cache, cloud placeholder detection, relink.
│   ├── render/     (clipforge-render)    wgpu compositor: Ken Burns, transitions, titles, colour pipeline (linear → SDR/HLG). Headless capable.
│   ├── export/     (clipforge-export)    Export planner (preset → ffmpeg args), sidecar process management, frame pump, progress.
│   ├── jobs/       (clipforge-jobs)      Job scheduler with priorities, cancellation, progress channels.
│   ├── platform/   (clipforge-platform)  OS glue: drag-and-drop payloads incl. file promises, native dialogs, OS thumbnails, cloud file status, app dirs.
│   ├── i18n/       (clipforge-i18n)      Catalogue loading, locale detection, shared with tests.
│   └── app/        (clipforge-app)       Slint UI, view models, adapters between UI and core. The only crate depending on Slint.
├── xtask/                     dev tasks: fixtures, packaging, licence report, i18n extraction
├── fixtures/                  tiny generated media (built by xtask, checked in when < 200 KB)
└── .github/workflows/
```

Dependency direction (enforced via `cargo deny` bans and a unit test that
parses the workspace graph):

```
app → {core, library, render, export, jobs, platform, i18n, media}
export → {core, render, media, jobs}
render → {core, media}
library → {core, media, jobs, platform}
media → {core}
platform, jobs, i18n, core → nothing internal
```

### 3.3 Core domain model (sketch)

```rust
struct Project { version, settings: ProjectSettings, media: MediaRefs, video: Sequence, audio: [AudioTrack; 2], titles: TitleSet }
struct ProjectSettings { aspect: Aspect, fps: FrameRate, default_photo_duration: Ticks, default_transition: TransitionSpec }
struct Sequence { clips: Vec<Clip>, transitions: Vec<Option<TransitionSpec>> }   // transitions.len() == clips.len() - 1
struct Clip { id: ClipId, media: MediaId, kind: PhotoClip | VideoClip, fit: Fit, rotate: Quarter, motion: KenBurns, caption: Option<Caption>, audio: ClipAudio }
struct PhotoClip { duration: Ticks }
struct VideoClip { in_point: Ticks, out_point: Ticks, speed: Ratio }
enum Command { InsertClips{..}, RemoveClips{..}, MoveClips{..}, SetDuration{selection, value}, SetTransition{selection, spec}, RandomiseTransitions{selection, seed}, SetKenBurns{..}, SetFit{..}, SetCaption{..}, FitToMusic{..}, ... }
struct History { undo: Vec<AppliedCommand>, redo: Vec<AppliedCommand> }
```

Derived data (clip start times, total duration, transition overlaps) is
computed by pure functions with memoisation in the view model, never stored.

Media identity: `MediaId` is a UUID owned by the library. A `MediaRef` in the
project carries the id plus a `Fingerprint { size, mtime, xxh3_of_first_and_last_1MiB }`
and last known path, so projects survive a moved library and can relink.

### 3.4 Media library

Schema (SQLite, WAL, `synchronous=NORMAL`, page size 8 KiB):

```
media(id, path, volume_id, file_id, size, mtime, fingerprint, kind, mime, width, height, duration_ticks, fps, rotation,
      captured_at, color_primaries, transfer, is_hdr, has_audio, cloud_state, probe_state, error, added_at)
media_fts(name, folder, caption)            -- FTS5, external content
thumb(media_id, level, path, w, h, generated_at)   -- levels: 128, 512, 1280 (preview), proxy video
folders(id, path, watch)                     -- optional folder watch
schema_version
```

Import pipeline (all Jobs):
1. **Enumerate** dropped paths (recursive), dedupe by fingerprint. Insert rows
   with `probe_state = pending`. Grid shows items immediately with a file-type
   placeholder.
2. **OS thumbnail** (fast path, no decode): Windows `IShellItemImageFactory`,
   macOS `QLThumbnailGenerator`. Works for cloud placeholders and RAW too.
   Level 128 available within tens of milliseconds per item.
3. **Probe** metadata with ffprobe-equivalent via libavformat / ImageIO /
   EXIF. Fills dimensions, duration, capture date, HDR flags.
4. **Own thumbnails** 512 and 1280 from the real file (skipped while the file
   is a non-hydrated cloud placeholder).
5. **Proxy video** (540p H.264, lazily, lowest priority, only for clips that
   are on a timeline or when playback drops frames).

Cache directory is content-addressed by fingerprint, so the same file linked
twice or moved does not re-generate. Cache size cap with LRU eviction.

Cloud placeholders: detect via `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` /
`FILE_ATTRIBUTE_OFFLINE` (Windows Cloud Files API) and
`NSURLUbiquitousItemDownloadingStatusKey` / `.icloud` stub files (macOS).
Hydration is an explicit Job ("Download 12 originals, 1.3 GB") started by
export or by the user, never implicitly by scrolling.

Drag-and-drop from Apple Photos delivers file promises (the Photos app exports
the file into a temp folder we provide). Those files are copied into a managed
`Imported from Photos` folder under the user's Pictures directory, because the
temp copy would otherwise vanish. This is the one case where ClipForge holds a
copy; the UI says so.

Performance budgets (checked by benches, tracked in CI as trends):
- Drop 5 000 files: rows visible in the grid < 1 s; first 128 px thumbnails for
  the visible viewport < 500 ms; full probe of 5 000 JPEGs < 60 s on M3 Max.
- Grid scroll at 60 fps with 20 000 rows.
- Timeline with 1 000 clips: any bulk command applied and UI updated < 50 ms.
- Project save < 50 ms for 1 000 clips.

### 3.5 Rendering pipeline

- Working space: linear light, Rec.2020 primaries, `rgba16float` textures.
- Source decode → colour transform into working space (SDR sources: BT.709 →
  2020, gamma removed; HLG/PQ sources: proper EOTF; Apple gain-map HEIC:
  base image + gain map applied via ImageIO's HDR decode).
- Ken Burns and fit are a single affine transform per clip; transitions are
  shaders taking two textures and a progress value; titles are rendered with
  `cosmic-text` into an atlas and composited.
- Output transform: SDR → BT.709 with a fixed tone-mapping operator (BT.2390
  EETF, tuned so SDR sources are bit-identical to their input) → 8-bit; HDR →
  HLG BT.2020 10-bit. HDR is opt-in in the export dialog.
- Preview renders at proxy resolution into a texture that Slint displays via
  the wgpu integration; export renders at full resolution and reads back to
  the encoder pipe. Same code path, different `RenderQuality`.
- Determinism: given the same inputs the compositor produces the same output
  across runs on the same GPU; golden tests use tolerance for cross-vendor
  differences.

### 3.6 Export

`ExportPlanner` turns `(Project, ExportOptions)` into an `EncodePlan` (pure,
snapshot-tested): resolution, pixel format, codec, encoder name after probing
ffmpeg for availability, colour tags, bitrate ladder, audio settings, container
flags (`+faststart` for YouTube), and file name.

Options mapping:
- 1080p/4K → 1920×1080 / 3840×2160 (or portrait). Good/Better/Best → bitrate
  ladder per resolution.
- YouTube → H.264 High for SDR (HEVC 10-bit HLG for HDR), AAC 384 kbps,
  `+faststart`, closed GOP 2 s.
- HDR → HEVC Main10, HLG (`arib-std-b67`), BT.2020, 10-bit; else H.264 8-bit
  BT.709 (HEVC selectable in Advanced).

The `Exporter` spawns the sidecar, pumps rendered frames and mixed audio
(PCM) over stdin/pipes, parses progress from ffmpeg's `-progress pipe:`,
handles cancel and failure, verifies the output opens and has the expected
duration.

### 3.7 UI (Slint)

Screens: Library (grid + inspector), Editor (preview + timeline + inspector +
library drawer), Export dialog, Settings. Layout mirrors Clipchamp so it feels
familiar.

The UI talks to `AppState` (in `app`, plain Rust) which owns the `Project`,
`History`, selection and view models. Slint callbacks map 1:1 to commands or
view state changes; no domain logic lives in `.slint` files or callbacks. The
view models are unit-tested without a window. The `.slint` layer is intended to
be replaceable (e.g. by egui) without touching anything below `app`.

Keyboard-first where it helps power users: Space, JKL, arrows to nudge, Cmd/
Ctrl+A, Delete, Cmd/Ctrl+Z/Shift+Z, Cmd/Ctrl+D duplicate, T for transition
palette.

## 4. Agentic setup

Delivered in Milestone 0 and maintained afterwards.

- **AGENTS.md** (`CLAUDE.md` is a symlink):
  - Purpose and one-paragraph architecture with the dependency arrows above.
  - Commands: `cargo test --workspace`, `cargo clippy --workspace --all-targets -D warnings`,
    `cargo fmt --check`, `cargo xtask fixtures`, `cargo xtask package`, `cargo bench`.
  - Rules: tests first (write the failing test, then code); no new dependency
    without a line in the ADR or `docs/DEPENDENCIES.md`; no `unwrap()` outside
    tests; every user-facing string goes through `@tr()`; core/render must
    stay free of platform and UI dependencies; no web technology; commands are
    the only way to mutate a project; every Job is cancellable.
  - Definition of done per change: tests, clippy clean, docs updated, i18n
    strings extracted, CI green on both OSes.
  - Where things live, how to add a transition / command / export option
    (short recipes).
  - Testing conventions: naming, fixture usage, golden-image tolerance, how to
    regenerate snapshots.
- **docs/adr/**: ADR-0001 Rust + Slint, ADR-0002 SQLite library, ADR-0003
  ffmpeg sidecar for encoding, ADR-0004 flicks as timebase, ADR-0005 colour
  pipeline, ADR-0006 command-based undo, ADR-0007 cloud placeholder handling.
- **docs/ARCHITECTURE.md**: redrawn from code as the system grows.
- **Issue templates / task files**: each milestone is broken into small tasks
  (`docs/tasks/M1-xx-*.md` or GitHub issues) an agent can complete in one
  session, each with acceptance tests listed.
- **CI as the reviewer**: fmt, clippy, tests, `cargo deny`, `cargo about`,
  bench regressions as trend (not blocking), packaging smoke test.
- **Fixtures generator** (`xtask fixtures`): produces tiny test media with
  ffmpeg (SDR/HLG videos, HEIC via macOS `sips` where available, JPEGs with
  EXIF dates, a fake cloud placeholder) so tests never depend on personal
  photos.

## 5. Testing strategy

Tests are written before the implementation for every piece of logic. Unit
tests dominate; integration tests cover the seams; a small end-to-end suite
protects the export path.

| Layer | What | How |
|---|---|---|
| core | Command apply/invert round-trips, sequence invariants (`transitions.len() == clips.len()-1`, no negative durations, transitions ≤ half of both neighbours), bulk ops, fit-to-music distribution, timing maths in flicks, project migrations | Unit tests + `proptest` (random command sequences must keep invariants; undo of any sequence restores the exact original) |
| media | Probe results for every fixture (dimensions, HDR flags, rotation, capture date); decoder trait conformance across backends; broken/truncated files produce errors not panics | Table-driven tests over `fixtures/`; backend tests gated by `cfg(target_os)` |
| library | Import pipeline on a temp dir, dedupe by fingerprint, relink after move, FTS search, cache eviction, cloud placeholder detection (mocked attributes), schema migrations from every previous version | In-memory SQLite + `tempfile`; time and filesystem behind small traits so tests are deterministic |
| render | Each transition at progress 0, 0.5, 1; Ken Burns transforms; colour round-trip (SDR in → SDR out bit-identical within ±1/255); HLG output for HDR fixtures; title layout | Headless wgpu (falls back to software adapter in CI); golden PNGs with perceptual tolerance, stored via `insta` |
| export | Planner snapshots for the full option matrix; sidecar arg construction; progress parsing; cancel; failure handling with a fake ffmpeg script | `insta` snapshots, fake sidecar |
| jobs | Priority ordering, cancellation, progress, no starvation | Unit tests with a manual executor |
| app | View models: selection, bulk-edit dispatch, undo labels, i18n switching, keyboard map | Unit tests without a window |
| e2e | Build a project from fixtures → export 1080p SDR and HDR → ffprobe verifies duration, codec, colour tags, frame count | One CI job per OS, nightly + on release branches |
| perf | Import 5 000 synthetic files, 1 000-clip bulk command, grid model with 20 000 rows | `criterion`, results uploaded as CI artefacts |

Additional safety nets: `cargo fuzz` targets for project-file parsing and the
ffmpeg progress parser; `#![forbid(unsafe_code)]` in every crate except
`media`/`platform` where FFI lives, with each `unsafe` block commented.

## 6. Milestones

Each milestone ends with something you can actually use. Order within a
milestone: tests → core → adapters → UI.

**M0 – Foundation (usable: an empty window that speaks two languages)**
- Workspace, crates with empty public APIs, AGENTS.md, ADR-0001..0007, README,
  LICENSES.md, CI matrix, `cargo deny`, `cargo about`, fixtures generator,
  i18n skeleton (en/de), Slint window with menu, Settings with language switch,
  packaging job producing `.app`/`.msi` artefacts on every CI run.

**M1 – Media library (usable: a very fast photo browser)**
- Drop files/folders, import pipeline, SQLite schema + migrations, OS
  thumbnails, own thumbnails, virtualised grid, sort/filter/search, inspector
  with metadata, cloud badges, relink, cache cap. Performance budgets met.

**M2 – Photo slideshow (usable: photos → MP4)**
- Core model, commands, undo/redo, selection model, timeline UI with thumbnail
  strips, drag to reorder, default duration, bulk set duration, fit modes,
  still-frame preview with scrubbing, export planner + sidecar, 1080p/4K SDR
  H.264 export with hard cuts only. First real slideshow.

**M3 – Videos (usable: mixed photo/video slideshows with sound)**
- Video decoding with hwaccel, audio decoding and mixing, realtime preview
  playback, proxies, trim handles, JKL, mute/volume, export with video audio.

**M4 – Motion and transitions (usable: it looks like a slideshow)**
- Transition library and shaders, per-transition duration, bulk set and
  randomise, Ken Burns presets and bulk apply, timing rules for overlaps,
  golden tests.

**M5 – Music and titles (usable: the intended product)**
- Audio tracks, fit-to-music, fades, ducking, loop; opening/closing titles,
  captions with styles and auto-fill, caption bulk ops.

**M6 – HDR and export polish (usable: the export dialog you described)**
- Colour pipeline finished, Apple ImageIO HDR photo decode, HLG video sources,
  HDR opt-in export via HEVC 10-bit HLG, YouTube preset, quality ladder,
  Advanced panel, output verification, e2e suite for SDR and HDR.

**M7 – Windows parity and packaging**
- Media Foundation hardware encoders via ffmpeg (`_mf`, NVENC/QSV/AMF probing),
  D3D11VA decode, Cloud Files placeholders, Explorer/Windows Photos drop
  payloads, installer, HEVC extension detection with a friendly hint, Fluent
  styling pass.

**M8 – Hardening**
- Crash recovery from auto-save, decoder watchdog, fuzz targets, memory
  ceilings for caches, long-session soak test (8 h playback/edit loop), perf
  regression review, accessibility pass, keyboard map documentation.

Post-v1 candidates (not planned): beat detection, filters, multi-track, PhotoKit
source, folder watching, presets sharing.

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Slint's wgpu integration or timeline performance turns out insufficient | The `app` crate is thin by design; egui is the fallback and can share the wgpu preview texture. Prototype the timeline with 1 000 clips in M2 before building more UI. |
| ffmpeg-next build pain on Windows CI | Pin a prebuilt LGPL/GPL ffmpeg via vcpkg or the BtbN builds; cache in CI; document in AGENTS.md. Decide in M0. |
| HDR correctness is subtle (gain maps, HLG on SDR displays, tone mapping) | Fixtures with known values, numeric colour tests, compare against Apple's own output on macOS, keep HDR opt-in. |
| RAW support balloons | v1 uses embedded JPEG previews for RAW; full demosaic is a post-v1 item. |
| Cloud placeholder detection differs per provider | Trait with per-OS implementation and a conservative default: unknown → treat as local. |
| File promises from Apple Photos are fiddly | Implement in `platform` behind a test double; manual test checklist in `docs/ux/`. |

## 8. Open questions for the refinement pass

1. Slint version and wgpu integration status at kickoff; verify the preview
   texture path with a spike before M2.
2. Should proxies for video be generated eagerly for anything on the timeline,
   or only on dropped frames? (Plan: lazily, lowest priority.)
3. Caption auto-fill: which EXIF fields and date format per locale?
4. Is a second audio track needed in v1 or is one music track enough?
5. Do we want folder watching (auto-import new files) in v1? (Plan: no.)
6. Minimum Windows version (plan: Windows 10 22H2+) and macOS version (plan:
   macOS 14+ for ImageIO HDR APIs).

## 9. Kickoff prompt

Use this to start development with a fresh context:

> Read `docs/PLAN.md` fully. Implement Milestone 0 exactly as described in
> sections 3.2, 4 and 6. Work test-first: for every crate create the public API
> skeleton with failing tests for the invariants named in section 5 that apply
> to that crate, then make them pass with the minimal implementation. Write
> `AGENTS.md` with the rules from section 4, symlink `CLAUDE.md` to it, and
> write ADR-0001 to ADR-0007 from section 2 in `docs/adr/`. Set up GitHub
> Actions for macOS and Windows running fmt, clippy, tests, cargo-deny,
> cargo-about and a packaging step. Add the `xtask fixtures` generator and
> commit the generated fixtures if they are under 200 KB total. The Slint app
> must open a window with a menu and a Settings page that switches between
> English and German at runtime. Do not start on Milestone 1. Finish with CI
> green on both operating systems and a short summary of what deviated from
> the plan and why.

## 10. Deviations log

Kept short; the ADRs hold the reasoning.

- **M0, renderer:** femtovg instead of Skia. Skia adds a large prebuilt
  binary download to every CI run and Slint's wgpu texture import is
  available through `renderer-femtovg-wgpu`, which is the path the preview
  needs anyway. Skia remains an option if text rendering quality disappoints.
- **M0, translations:** Slint's bundled `.po` translations instead of
  gettext at runtime, to avoid a native libintl dependency on Windows.
  Runtime switching works via `slint::select_bundled_translation`.
- **M0, fixtures:** no WebP fixture; the local ffmpeg build lacks libwebp.
  A TIFF fixture took its place. WebP decoding is still in scope and will be
  covered via the `image` crate with an in-test generated file.
- **M0, packaging:** cargo-packager formats are passed on the command line
  per OS because the config schema does not allow per-platform `formats`.
- **M1, probing:** video/audio metadata and video frames come from the
  `ffprobe`/`ffmpeg` executables (process-isolated, timeout-guarded) rather
  than in-process libav. In-process decoding is still planned for playback
  in M3 (ADR-0003); for the library the sidecar is simpler and safer.
- **M1, OS thumbnails:** not implemented yet. The own pipeline turned out fast
  enough (5 000 JPEGs in 1.8 s) that the Quick Look / Shell fast path is
  deferred until placeholders or RAW files make it necessary.
- **M1, drag-and-drop:** Slint 1.18 does not surface OS file drops, but its
  winit backend exposes the raw winit events. `app/src/drop.rs` hooks
  `DroppedFile`/`HoveredFile` there (batched into one import), which works on
  macOS and Windows without platform code.
- **M1, thumbnail levels:** 256 / 640 / 1280 instead of 128 / 512 / 1280 to
  match Retina grid cells and the inspector.
