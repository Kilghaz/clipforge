# Architecture (living document)

Redrawn from the code after each milestone. Last update: Milestone 1 (library).

## Crates

| Crate | Purpose | Key types today |
|---|---|---|
| `clipforge-core` | Model, commands, undo, time | `Ticks`, `FrameRate`, `Aspect`, `Resolution`, `MediaId` |
| `clipforge-media` | Probing and still decoding | `MediaInfo`, `Rotation`, `Prober`, `StillDecoder`, `ImageBackend`, `FfmpegCli`, `Backends` |
| `clipforge-library` | Catalogue, cache, import jobs | `Catalogue`, `Query`, `MediaRecord`, `ThumbCache`, `Library`, `LibraryEvent` |
| `clipforge-render` | Compositor | `RenderQuality` |
| `clipforge-export` | Export planning and sidecar | `ExportOptions`, `EncodePlan` |
| `clipforge-jobs` | Background work | `Scheduler`, `Priority`, `CancellationToken`, `Progress`, `JobEvent` |
| `clipforge-platform` | OS glue | `AppDirs`, `cloud_status`, `icloud_stub`, `reveal_in_file_manager` |
| `clipforge-i18n` | Languages | `Language`, `LanguagePreference` |
| `clipforge-app` | Slint UI | `MainWindow`, `LibraryState` (Slint global), `LibraryController`, `SettingsStore` |
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

## Runtime shape (target, from Milestone 2)

```
UI thread (Slint)  ── callbacks ──►  AppState { Project, History, Selection }
        ▲                                   │ commands
        │ view models / preview texture     ▼
   Job scheduler ◄── jobs ──  library (import, thumbs, proxies)
        │                     render (preview frames on GPU)
        └── export ── frames/PCM over pipes ──► ffmpeg sidecar process
```

## Data at rest

- Settings: `<config>/settings.json` (atomic write).
- Library: `<data>/library.sqlite` (WAL, FTS5 trigram index on names).
- Cache: `<cache>/thumbs/<hh>/<fingerprint>/{256,640,1280}.jpg`; proxies from M3.
- Project: single JSON file chosen by the user (from M2).
