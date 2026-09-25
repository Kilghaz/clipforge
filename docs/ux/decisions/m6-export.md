# M6 export dialog — behaviour decisions

The user wants to export the slideshow with a few understandable choices
(resolution, quality, HDR, YouTube), reach the technical knobs when needed,
and keep working while it encodes.

References read: DESIGN.md §1 "Dialogs", "Long-running work"; Fluent
"Dialogs"; Primer "Progressive disclosure" (chevron + text trigger, used
sparingly, keep context); NN/G "Progress indicators" (percent done for
long tasks, approximate time left, let users stop), NN/G "Visibility of
system status".

## Content, top to bottom

| Element | Component | Decision |
|---|---|---|
| Resolution | `ChoiceGroup` | unchanged: Full HD / 4K |
| Quality | `ChoiceGroup` | Good / Better / Best, plus a `Caption` summary under it: "About 240 MB · H.264 · 12 Mbit/s" (size from bitrates × show length). The summary answers "what does Best mean" without a manual. |
| Optimise for YouTube | `Switch` | unchanged; forces MP4, closed 2 s GOPs, AAC 384 kbit/s |
| HDR | `Switch` "HDR (for iPhone and YouTube HDR)" | off by default. Enabled only with an HLG/PQ video on the timeline *and* a GPU renderer. A caption always says why: no HDR videos / needs a supported graphics card / what HDR produces ("HEVC 10-bit HLG. Plays in HDR on iPhone, Mac and YouTube; SDR photos and texts keep their look."). Turning it on is remembered only for this session. |
| Advanced | disclosure row: chevron + "Advanced" (Primer) | collapsed by default, remembers its state for the session. Accessible as a button with expanded state in its label. |
| Advanced: Codec | std `ComboBox` | Automatic / H.264 / HEVC. With HDR on: disabled, caption "HDR is always HEVC". |
| Advanced: Frame rate | std `ComboBox` | "Project (30 fps)" / 24 / 25 / 30 / 50 / 60 |
| Advanced: Video bitrate | std `ComboBox` | "Automatic (12 Mbit/s)" plus 8 / 12 / 16 / 20 / 35 / 50 / 80 Mbit/s |
| Advanced: Audio bitrate | std `ComboBox` | "Automatic (256 kbit/s)" / 128 / 192 / 256 / 320 / 384 |
| Advanced: File format | std `ComboBox` | MP4 / MOV. With YouTube on: disabled at MP4. |

Advanced uses label/value rows (a two-column grid) rather than
`ChoiceGroup`s: five settings stacked as button groups would double the
dialog height, and ComboBoxes are what Fluent and Spectrum use for
settings forms with several values.

## Running export

- **Feedback:** determinate `ProgressIndicator`, "Exporting… 42 %", and
  after ~3 s and 2 % an approximate time left ("About 3 min left", "Less
  than a minute left"), NN/G wording: approximate, never seconds counting.
- **Keep working:** a "Keep editing" button (the dialog's primary while
  exporting) closes the dialog; the export runs on. The toolbar's Export
  button turns into a progress button ("42 %" with a small progress bar);
  clicking it reopens the dialog. Edits made meanwhile are not in the
  export (the project was copied at start); the dialog says so.
- **Cancel:** "Cancel export" stays the safe button while running. Esc
  only closes the dialog (keeps exporting) — a stray Esc must not throw
  away minutes of work.
- **Completion:** the dialog (or, if closed, the toolbar button) shows
  success: check icon, "Saved movie.mp4 (240 MB)" and a "Show in
  Finder" / "Show in Explorer" button. The toolbar button shows a check
  until the dialog is opened. Right after success "Close" becomes the
  default button (Enter) and "Export again…" the secondary one, until an
  option changes. No OS notification yet (deferred, PLAN
  deviations).
- **Verification:** after encoding, ffprobe checks codec, size, duration,
  colour tags and audio. A mismatch keeps the file and shows a warning
  ("Saved movie.mp4, but the file differs from the plan: …").
- **Errors:** inline icon + message, as before; HDR without GPU at run time
  ("HDR frames need the GPU renderer…") uses the same path.

## Other decisions

- Selection / undo: none — export does not change the project.
- Keyboard: Tab order follows the visual order; Space toggles switches and
  the disclosure; Enter starts the export (the dialog's default action),
  not while one runs.
- Empty state: the Export button is disabled with an empty timeline
  (unchanged).
- German: "Für YouTube optimieren", "Erweitert", "Weiter bearbeiten",
  "Export abbrechen"; the summary caption wraps.
- Platform: button order via `DialogButtons` (macOS trailing default);
  "Show in Finder" vs "Show in Explorer".
- Screenshot scenes: `export` (idle, Advanced open, HDR available),
  `export-running` (progress with time left), `export-done`,
  `export-background` (toolbar), `narrow-export` (900 × 560, scrolling);
  gallery rows for the disclosure row and the toolbar status button.
- Dialog height: at 900 × 560 the dialog would not fit with Advanced open,
  so `DialogFrame` caps its height and scrolls the body with a fixed button
  row (Fluent keeps the buttons visible).

## Tests first (view model, `export_view.rs`)

- `options_follow_the_dialog_state` (indices → `ExportOptions`, Advanced
  "Automatic" → `None`)
- `hdr_availability_explains_why_it_is_off`
- `summary_shows_size_codec_and_bitrate` (and HDR → HEVC, YouTube → MP4)
- `time_left_is_approximate_and_waits_for_data`
- conflicts (HDR → HEVC, YouTube → MP4) are resolved by the planner:
  `plan::tests::hdr_and_youtube_win_over_conflicting_advanced_choices`;
  the dialog shows them as disabled pickers
- `ui_interaction::the_export_dialog_discloses_advanced_and_gates_hdr`
