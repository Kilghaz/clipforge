# Architecture (living document)

Redrawn from the code after each milestone. Last update: Milestone 0.

## Crates

| Crate | Purpose | Key types today |
|---|---|---|
| `clipforge-core` | Model, commands, undo, time | `Ticks`, `FrameRate`, `Aspect`, `Resolution` |
| `clipforge-media` | Media vocabulary, decoder traits | `MediaKind`, `ColorTransfer` |
| `clipforge-library` | Catalogue and cache | `Fingerprint` |
| `clipforge-render` | Compositor | `RenderQuality` |
| `clipforge-export` | Export planning and sidecar | `ExportOptions`, `EncodePlan` |
| `clipforge-jobs` | Background work vocabulary | `Priority`, `CancellationToken`, `Progress` |
| `clipforge-platform` | OS glue | `AppDirs` |
| `clipforge-i18n` | Languages | `Language`, `LanguagePreference` |
| `clipforge-app` | Slint UI | `MainWindow`, `SettingsStore` |
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
- Library: `<data>/library.sqlite` (from M1).
- Cache: `<cache>/<fingerprint>/{128,512,1280}.jpg`, `proxy.mp4` (from M1).
- Project: single JSON file chosen by the user (from M2).
