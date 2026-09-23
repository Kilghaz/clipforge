# Architecture (living document)

Redrawn from the code after each milestone. Last update: Milestone 2 (photo slideshow).

## Crates

| Crate | Purpose | Key types today |
|---|---|---|
| `clipforge-core` | Model, commands, undo, time | `Project`, `Clip`, `Command`, `History`, `timeline::{placements, frame_at}`, `Ticks`, `FrameRate` |
| `clipforge-media` | Probing and still decoding | `MediaInfo`, `Rotation`, `Prober`, `StillDecoder`, `ImageBackend`, `FfmpegCli`, `Backends` |
| `clipforge-library` | Catalogue, cache, import jobs | `Catalogue`, `Query`, `MediaRecord`, `ThumbCache`, `Library`, `LibraryEvent` |
| `clipforge-render` | CPU compositor | `Compositor`, `Frame`, `SourceProvider`, `layout::place`, `RenderQuality` |
| `clipforge-export` | Planner, frames, ffmpeg sidecar | `EncodePlan`, `Exporter`, `TimelineFrames`, `FileSources`, `EncoderCatalog`, `Yuv420` |
| `clipforge-jobs` | Background work | `Scheduler`, `Priority`, `CancellationToken`, `Progress`, `JobEvent` |
| `clipforge-platform` | OS glue | `AppDirs`, `cloud_status`, `icloud_stub`, `reveal_in_file_manager` |
| `clipforge-i18n` | Languages | `Language`, `LanguagePreference` |
| `clipforge-app` | Slint UI | `MainWindow` (library panel + editor side by side, settings overlay), `LibraryState`/`EditorState`/`Shell` (Slint globals), `LibraryController`, `EditorController`, `editor_view` (pure), `SettingsStore` |
| `xtask` | Dev tasks | `check-deps`, `fixtures`, `icons` |

## Dependency graph

```
                 ┌────────── app ──────────┐
                 │      │      │     │     │
              export  library  render i18n platform
               │  │     │  │     │
             render jobs media jobs media
               │          │
             core ◄───────┘
```

`core`, `jobs`, `platform`, `i18n` have no internal dependencies.

## Library pipeline (Milestone 1)

```
drop / picker ──► Library::import (job, Soon)
   enumerate (walkdir, no file reads) ─► prepare (fingerprint head+tail 1 MiB;
   placeholders get a provisional fingerprint) ─► Catalogue::add_many (200/tx)
   ─► LibraryEvent::ItemsAdded ─► probe jobs (Background, 25 items each)
        probe: Backends::probe ─► Catalogue::set_info / set_probe_failed
               ─► ItemUpdated ─► eager Small thumbnail into ThumbCache
UI cell shown ──► Library::request_thumb(id, level, Interactive)
   cached? path now : job (dedupe + reprioritise) ─► ThumbReady / ThumbFailed
```

Rules baked in: placeholders are never read; audio, failed probes and
thumbnails that failed this session are not retried; the cache is
content-addressed by fingerprint and trimmed to 2 GB at startup.

Measured on an M3 Max (release, `perf_import_5000_jpegs`): 5 000 files
registered in 0.44 s, probed with small thumbnails in 1.8 s, `query_ids`
over 5 000 rows in 1.1 ms.

## Editing and export (Milestone 2)

```
Slint EditorState ── callbacks ──► EditorController { Project, History, Selection }
        ▲                                 │ Command::apply → inverse pushed to History
        │ TimelineClip model, preview     ▼
        │ image (960x540 RGBA)      Compositor::render(project, playhead, Preview,
        │                                 PreviewSources ← library 1280 px thumbs)
        │
   Export… ──► job: TimelineFrames(project, Compositor, FileSources(full decode))
               ──► Yuv420 per distinct picture ──► ffmpeg stdin ──► .mp4
               progress via -progress pipe, cancel kills the process
```

Rules: the project changes only through `History::apply`; bulk edits with
an empty selection apply to every clip and also become the project's
defaults for newly added clips; a photo clip outside a transition is
rendered once and its yuv buffer re-sent for every frame; autosave to
`<data>/autosave.clipforge.json` three seconds after the last change.

## Data at rest

- Settings: `<config>/settings.json` (atomic write).
- Library: `<data>/library.sqlite` (WAL, FTS5 trigram index on names).
- Cache: `<cache>/thumbs/<hh>/<fingerprint>/{256,640,1280}.jpg`; proxies from M3.
- Project: single JSON file (`*.clipforge.json`) chosen by the user; unused media refs are pruned on save.
- Autosave: `<data>/autosave.clipforge.json`, restored on start.
