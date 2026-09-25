# Decisions: transitions and motion (Milestone 4)

Written with `/ux-prepare` before implementation; `/ux-review` checks
against it.

## Task

"The user wants to give a slideshow of a hundred photos varied transitions
and gentle movement in a few clicks, and adjust single clips where one looks
wrong."

## Sources read (2026-09-25)

| Source | Page | Rule taken |
|---|---|---|
| NN/G | https://www.nngroup.com/videos/bulk-actions-design-guidelines/ | select all, contextual actions, clear feedback, undo |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Picker.html | single choice in limited space → picker; every picker has a label; group long lists |
| Fluent 2 | https://learn.microsoft.com/en-us/windows/apps/design/controls/combo-box | drop-down for single-line choices; ≥ 5 items (else radio); sort logically, related options together, most common first; type-to-search built in |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionButton.html | action buttons for "similar, task-based options within a workflow", visible label, quiet when not the main action |
| Primer | Content guidelines (DESIGN.md §3 text rules) | sentence case, verb-first actions |

## Components

| Element | Choice | Why |
|---|---|---|
| Transition kind | std `ComboBox` (13 items) | Fluent: ≥ 5 single-line options → drop-down. Order: Cut, Cross dissolve, Fade through black, Fade through white, then slides (left, right, up, down), wipes (same order), Zoom. Slint's ComboBox has no section headers; grouping is by adjacency and naming ("Slide left", "Wipe left"). |
| Transition duration | existing `ValueSlider` | unchanged |
| Motion preset | std `ComboBox` (7 items) | same rule; None first, zooms, then pans. |
| Shuffle transitions / Shuffle motion | existing `ActionButton` with a new Fluent icon `arrow_shuffle` | Spectrum action button; the verb names the action; scope comes from the inspector's context line ("Changes apply to the selection" / "…to all N clips"). |
| Motion badge on a clip | existing `Badge` (icon-only variant used by "Muted") | Spectrum badge; shows that a photo moves without opening the inspector. |
| Transition overlay on a clip | existing overlay, one neutral colour for every movement kind, `title-clip` for dissolves and fades | 13 kinds cannot be told apart by colour; the inspector names the kind. |

No new component; the spec cards for `timeline-clip` and `std-widgets` get
the new states, the gallery gets a clip with a motion badge.

## Behaviour

- **Scope / bulk:** as every inspector control today: the selection, or all
  clips when nothing is selected (then the value also becomes the default
  for clips added later). Shuffle with nothing selected shuffles all clips
  and does not change the defaults (a shuffle is a one-off).
- **Photos only for motion:** the Motion section is shown when the targets
  include photos (or the timeline is empty, for the default). With a mixed
  selection it applies to the photos and says so: "Applies to photos only".
- **Undo:** one step per user action. Picker/duration → `SetTransition`
  (+ `SetSettings` for defaults), shuffle → `SetTransitionEach`, motion
  picker → `SetMotion` (+ `SetSettings`), motion shuffle → `SetMotionEach`.
- **Feedback (< 100 ms):** the timeline overlays/badges and the preview
  update on the same frame; the status line reports "Shuffled transitions on
  N clips" / "Shuffled motion on N photos".
- **Timing rules made visible:** a transition never overlaps more than half
  of either neighbour. When the chosen duration is longer than that for any
  target, a caption under the slider says "Shortened where clips are too
  short", so the user knows why an overlap looks smaller.
- **Keyboard:** everything is Tab-reachable in the inspector; the pickers
  support arrow keys and type-to-search. `T` moves focus to the transition
  picker (planned in `keyboard.md` since M0).
- **Empty timeline:** pickers set the defaults for new clips; the shuffle
  buttons are disabled (nothing to shuffle).
- **Errors:** none possible from the user's side (all values valid); a
  rejected command is logged and changes nothing.
- **Wording (EN / DE):** "Transition into clip" / "Übergang in den Clip",
  "Shuffle transitions" / "Übergänge mischen", "Motion" / "Bewegung",
  "Shuffle motion" / "Bewegung mischen", presets "Zoom in / Hineinzoomen",
  "Pan left / Nach links schwenken", … German runs ~30 % longer; the
  inspector is 296 px, pickers stretch to the full width.
- **Platform:** no difference; std widgets follow fluent/cupertino.
- **Screenshot scene:** `populated` gets a dissolve, a slide and a moving
  photo so the overlay and badge are visible; the inspector in `selection`
  shows the Motion section.

## Tests first (view model, `editor_view.rs`)

- `photo_targets_skip_videos`
- `capped_transition_is_reported_when_a_neighbour_is_too_short`
- `uncapped_transition_is_not_reported`
- `clip_view_flags_follow_motion_and_transition` (moving / transition index)

## Review (`/ux-review`, 2026-09-25)

Renders looked at: `populated.png` (+ crops of timeline, inspector and
transport bar), `narrow.png` (+ transport crop), `empty.png`, `gallery.png`
(overview + the M4 clip row at 2×). Tests: `cargo nextest run -p
clipforge-app` 34/34 including token lint and gallery coverage; clippy clean.

Fixed in this pass:

1. **should** — badges were hidden under the next clip's overlap whenever a
   transition followed (top-right slot); moved to the left after the clip's
   own overlay. Affected the existing mute badge as well.
2. **should** — the movement-transition wash was too faint over bright
   footage (DESIGN §2, UI elements ≥ 3:1); added a light-on-dark end edge,
   kept out of the footer so it never cuts through the clip name.
3. **should** — `status-text` was not displayed anywhere, so the promised
   "Shuffled … on N" feedback (and the older "Added N items") was invisible;
   now a caption in the transport bar for 5 s. The time read-out keeps its
   width in narrow windows; the status gives way.
4. **should** — spec cards (`timeline-clip`, `badge`, `std-widgets`), the
   DESIGN §4 component table, `keyboard.md` and `manual-checks.md` updated.

Open for triage (not changed):

- **should** — std ComboBox has no type-to-search; the transition list now
  has 13 items (Fluent: text search helps long lists). Needs a custom picker.
- **nice** — `T` is not shown in a menu or tooltip (same as J/K/L).
- **nice** — badge tooltip partly under the trim handle on videos without a
  transition; badge clipped on minimum-width clips with long overlaps.
- **nice** — "Ken Burns" in the badge tooltip is jargon.

