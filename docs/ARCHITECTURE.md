# Architecture (living document)

Redrawn from the code after each milestone. Last update: Milestone 5 (music and titles).

## Crates

| Crate | Purpose | Key types today |
|---|---|---|
| `clipforge-core` | Model, commands, undo, time | `Project`, `Clip` (photo / video / colour card), `TextItem` (text track), `TransitionKind`, `Motion`, `Music` (`Song`, `song_spans`, `MusicEnvelope`), `Command`, `History` (with merge groups), `shuffle`, `timeline::{placements, frame_at, opening_overlap}`, `Ticks`, `FrameRate` |
| `clipforge-media` | Probing, stills, streaming decode | `MediaInfo`, `Prober`, `StillDecoder`, `ImageBackend`, `FfmpegCli` (+`frame_at`), `VideoReader`, `AudioReader`, `Backends` |
| `clipforge-library` | Catalogue, cache, import jobs | `Catalogue`, `Query`, `MediaRecord`, `ThumbCache`, `Library`, `LibraryEvent` |
| `clipforge-render` | GPU compositor (wgpu) with CPU fallback (ADR-0010), caption text | `FrameRenderer`, `best_renderer`, `GpuCompositor`, `Compositor`, `text::TextRenderer`, `draw::draw`, `transition::apply`, `Frame`, `SourceProvider`, `layout::place`, `RenderQuality` |
| `clipforge-export` | Planner, frames, audio mix, ffmpeg sidecar | `EncodePlan`, `Exporter`, `TimelineFrames`, `FileSources`, `audio::{Mixer, mix, write_mix}`, `EncoderCatalog`, `Yuv420` |
| `clipforge-jobs` | Background work | `Scheduler`, `Priority`, `CancellationToken`, `Progress`, `JobEvent` |
| `clipforge-platform` | OS glue | `AppDirs`, `cloud_status`, `icloud_stub`, `reveal_in_file_manager` |
| `clipforge-i18n` | Languages | `Language`, `LanguagePreference` |
| `clipforge-app` | Slint UI | `MainWindow` (library panel + editor, settings overlay), `LibraryState`/`EditorState`/`Shell` (Slint globals), `LibraryController`, `EditorController`, `Player` (frame fetchers + cpal audio), `editor_view`/`library_view` (pure), `SettingsStore` |
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
        │ image (960x540 RGBA)      best_renderer().render(project, playhead, Preview,
        │                                 PreviewSources ← library 1280 px thumbs)
        │
   Export… ──► job: TimelineFrames(project, best_renderer(), FileSources(full decode))
               ──► Yuv420 per distinct picture ──► ffmpeg stdin ──► .mp4
               progress via -progress pipe, cancel kills the process
```

Rules: the project changes only through `History::apply`; bulk edits with
an empty selection apply to every clip and also become the project's
defaults for newly added clips; a photo clip outside a transition is
rendered once and its yuv buffer re-sent for every frame; autosave to
`<data>/autosave.clipforge.json` three seconds after the last change.

## Transitions and motion (Milestone 4)

```
Clip.transition_in: Transition { kind: TransitionKind (13), duration }
Clip.motion: Motion (None, ZoomIn/Out, Pan×4) — photos only
compositor: frame_at(t) ─► render_clip(current) [+ render_clip(outgoing)]
            render_clip: place(fit) ─► Motion::camera(progress) zoom/pan
                         ─► draw(src, rect)   (only the visible source part is resampled)
            transition::apply(kind, from, to, progress)  (slides/wipes/zoom eased)
            first clip: opening_overlap ─► apply(kind, solid black|white, frame)
bulk: Command::SetTransition / SetMotion (same value), SetTransitionEach /
      SetMotionEach (per clip, from shuffle::{transitions, motions}(seed))
```

Rules: overlaps are capped at half of either neighbour (the inspector says
"Shortened where clips are too short"); a shuffle never gives two
neighbouring clips the same kind; export re-renders every frame of moving
photos and of the opening, and reuses the frame of still photos.

## Data at rest

- Settings: `<config>/settings.json` (atomic write).
- Library: `<data>/library.sqlite` (WAL, FTS5 trigram index on names).
- Cache: `<cache>/thumbs/<hh>/<fingerprint>/{256,640,1280}.jpg`; proxies from M3.
- Project: single JSON file (`*.clipforge.json`) chosen by the user; unused media refs are pruned on save.
- Autosave: `<data>/autosave.clipforge.json`, restored on start.

## GPU compositor (ADR-0010)

```
best_renderer() ─► GpuCompositor (wgpu: Metal / DX12 / Vulkan)
                   └─ no adapter or CLIPFORGE_RENDERER=cpu ─► Compositor (CPU)
GpuCompositor::render: clip ─► quad into target A (fit, rotation, camera)
                       outgoing clip or opening colour ─► target B
                       transition pass (alpha / offset / scissor / solid) ─► out
                       ─► read back RGBA ─► Frame (same as the CPU path)
caches: stills with mips (LRU, 768 MB), one texture per video (re-upload on new frame)
```

`tests/gpu_matches_cpu.rs` keeps both compositors in step; it skips when
no adapter is available.

## Music and titles (Milestone 5)

```
Project.music: Music { songs: [Song { id, media }], volume, fade_in, fade_out, looped, duck }
   music::song_spans(project, show_end)  songs back to back from 0, looped, cut at the end
   music::MusicEnvelope                  volume × fades × ducking under video sound
export::audio::Mixer(project, sources, from)
   voices = video clips (transition cross-fades) + song spans (envelope), opened lazily
   ├─ export: write_mix → WAV (streamed, cancellable) → ffmpeg
   └─ preview: player feed thread → cpal ring buffer
ClipSource::Title { duration, background }   colour card on the video track
Project.texts: [TextItem { text, start, duration, x, y, width, style, enter, exit }]
render::text::TextRenderer (parley + swash, six bundled OFL fonts, no system fonts)
   text block image cached by (text, style, box width, frame size); position applied at draw
   text_draw(item, t) → rect, alpha, clip (fade / slide / wipe / zoom in and out)
   both compositors draw the text track over the finished frame (after transitions)
app: TextLane (time), TextOverlay (WYSIWYG on the preview), text_view (pure gestures)
```

Commands: `SetMusic` (songs and settings in one), `InsertTexts`,
`RemoveTexts`, `SetTexts` (move, resize, restyle, retime any number of texts
in one step), `SetTitleBackground`; colour cards are clips inserted with
`InsertClips` and count as stills for `SetPhotoDuration`. Typing a text uses
`History::apply_merging`, so a typing session is one undo step. Projects
saved with the earlier per-clip captions are converted to texts on load.
