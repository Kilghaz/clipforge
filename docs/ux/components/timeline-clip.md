# Timeline clip and strip (spec card)

Status: audited 2026-09-24, updated 2026-09-25 (M4: motion badge, transition catalogue, badge placement, overlay edge)
Implementation: `crates/app/ui/editor.slint` → `ClipView` (clip card, transition overlay, mute badge, trim handles) and the `strip` section of `EditorPage` (ruler, playhead, trim marker, drop marker, drop-zone tint, zoom slider in the transport `Bar`, `FocusScope keys`); geometry and selection in `crates/app/src/editor_view.rs` (`layout`, `drop_index`, `time_at_x`, `x_at_time`, `Selection`, `MIN_CLIP_WIDTH`); gestures in `crates/app/src/editor_ui.rs` (`clip_pressed` / `clip_dragged` / `clip_released`, `trim_for` / `trim_dragged` / `trim_released`, `scrub`, `step_frames`, `library_drop_hover`).
Gallery rows: "Timeline clips (100 px): photo, selected photo with dissolve, video, muted video, no thumb" and "Timeline clips, M4: moving photo (no transition), moving photo with slide, video with wipe (movement overlay), selected moving photo with zoom" in `crates/app/ui/gallery.slint`. The ruler, playhead and markers have no gallery row; they are visible in `target/screenshots/populated.png`.

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| NN/G | https://www.nngroup.com/articles/direct-manipulation/ | "physical, incremental, and reversible actions whose effects are immediately visible"; keyboard alternatives for precision; undo |
| NN/G | https://www.nngroup.com/articles/drag-drop/ | cursor change on hover, ghost, centre-overlap reshuffle threshold, ~100 ms reflow animation, keyboard alternative |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ListView.html (drag and drop section; `Card.html` returns 404) | drop positions before/after/on/root, `onReorder`, move vs copy, highlight selection with double-click action |
| Spectrum (React Aria) | https://react-aria.adobe.com/dnd and https://react-aria.adobe.com/GridList | drop indicator "rendered between items", drag preview with count badge, Enter starts keyboard drag / arrows pick target / Escape cancels, Escape clears selection |
| Apple HIG | https://developer.apple.com/design/human-interface-guidelines/playing-video (read via the `tutorials/data/.../playing-video.json` payload) | "people expect to press Space ... to play or pause"; original aspect ratio; thumbnail track for scrubbing |
| Apple HIG | https://developer.apple.com/design/human-interface-guidelines/drag-and-drop (json payload) | drag image after ~3 pt, translucent, insertion point only when destination accepts, undo drops |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/input/drag-and-drop | DragUI content/glyph/caption, DragOver accepted operation, ListView `CanReorderItems` |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/input/keyboard-interactions | arrow inner navigation, Home/End, Space/Enter, Esc cancels ongoing action, Ctrl++ / Ctrl+- zoom, focus visual |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/menus-and-context-menus, https://learn.microsoft.com/en-us/windows/apps/design/controls/tooltips | context menu on content elements; tooltip for information not visible on screen |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Clip card | Spectrum card: preview + footer; DESIGN §2 card radius 6 | `ClipView` `radius-m`, 1 px `border`, `surface-card`, `clip: true` | ✅ | gallery "Timeline clips" |
| Type colour | DESIGN §2 semantic tokens `photo` / `video`, ≥ 3:1, not red/green only | 3 px stripe along the top edge, `Theme.photo` (blue) or `Theme.video` (purple) | ✅ | gallery photo vs. video |
| Thumbnail | Apple: display at original aspect ratio (player); Spectrum preview | `image-fit: cover` over `surface-canvas`; fallback kind icon 28 px `text-disabled` | ✅ (cover is fine for a strip thumbnail; the preview pane uses `contain`) | gallery "no thumb" |
| Label footer | Spectrum card footer; DESIGN: "solid label footer" | 22 px `surface-card` footer, title 11 px `elide` + duration `Caption` | ✅ | gallery |
| Duration | Premiere-style read-out | `format::duration(clip.duration())` right-aligned | ✅ | gallery "4.0 s", "0:06" |
| Transition overlay | DESIGN §4 "transition overlay"; DESIGN §2 UI elements ≥ 3:1 | `Rectangle` over the overlap (min 10 px): blending kinds (dissolve, fade through black/white) = `title-clip` 40 %, movement kinds (slides, wipes, zoom) = `text` 20 %; the overlap's end is marked by a 1 px `text` line on a 3 px `badge-backdrop`, inset 4 px from the top and stopping 4 px above the footer. The 13 kinds are not told apart by colour; the inspector names the kind | ✅ (the wash alone was too faint over bright footage; the edge reads on any picture) | gallery "M4" row, `populated.png` |
| Mute badge | Spectrum Badge | `Badge` `speaker-mute` "Muted", videos only, at the left just after the clip's own transition overlay (`badge-x`) | ✅ | gallery "muted video" |
| Motion badge | Spectrum Badge; decision note `docs/ux/decisions/m4-motion.md` | `Badge` `arrow-move` "Moves (Ken Burns)", photos with a motion preset, same slot as the mute badge (the two never coexist) | ✅ | gallery "M4" row |
| Title card (M5) | decision note `docs/ux/decisions/m5-music-titles.md` | background colour (`title-bg-*`, same values as the renderer) instead of a thumbnail, the first text line centred 12 px semibold (dark text on the white card), `title-clip` stripe, footer "Title card" | ✅ | gallery "Timeline clips, M5" |
| Caption badge (M5) | Spectrum Badge; NN/G icon usability (tooltip) | `Badge` `closed-caption` "Has a caption" on photos and videos with a caption, one slot right of the motion / mute badge; not on title cards (the card shows its text) | ✅ | gallery "Timeline clips, M5" |
| Badge slot | Fluent info badge: inside the parent, top-right | left, after the clip's own overlay: the right end of a clip is covered by the next clip's overlap, which hid top-right badges whenever a transition followed | ➖ deliberate deviation (reason given) | `populated.png` before/after 2026-09-25 |
| Trim handles | NN/G: affordance visible; Premiere edge handles | 10 px `TouchArea` each edge, 4 px `text` bar at 25 % opacity, 70 % on hover/selected, videos only | ✅ (see metrics for hit width) | manual-checks M3 |
| Ruler | DESIGN §4 "one tick per second, labels thinned by zoom" | 24 px `surface-pane` ruler, 1 px tick per second (8 px at label step, 4 px otherwise), `m:ss` caption at `label-step` 1/2/5/10 s | ✅ | populated.png |
| Playhead | Apple: scrubber; DESIGN token `playhead` | 2 px `Theme.playhead` line full strip height + 12 × 12 grab head in the ruler | ✅ | populated.png |
| Drop insertion marker | React Aria: "DropIndicator is rendered between items"; Apple: insertion point only when destination accepts | 3 px `Theme.accent` line at `boxes[to].x`, spanning clip height + 8 px | ✅ (note: manual-checks.md still calls it "orange"; code uses accent) | `library_drop_hover`, `clip_dragged` |
| Live trim | NN/G direct manipulation: "effects immediately visible"; the edge stays under the pointer | the dragged edge follows the cursor exactly (`editor_view::trim_at_cursor` from a `TrimAnchor` taken at the first move); right edge: later clips shift live; left edge: the right edge stays put and the gap closes on release; clamped at the source length and 0.2 s; below 56 px the minimum width wins. The former trim marker line is gone | ✅ | tests `right_edge_follows_the_cursor_exactly`, `left_edge_follows_the_cursor_and_keeps_the_right_edge` |
| Drop-zone tint | Fluent DragOver feedback; Apple highlight only while above | `drop-target` fill + 2 px accent border over the strip while `Shell.drag-over-timeline` | ✅ | manual-checks "Design guide pass" |
| Drag ghost (clip reorder) | Fluent DragUI content; Apple translucent drag image; React Aria "copy of the dragged element" | none for clip reordering: clips stay in place, only the drop marker moves | ❌ | |
| Zoom control | Fluent Ctrl++/−; Premiere zoom slider | `Slider` 10–200 px/s between zoom-out/zoom-in icons, `Tooltip` "Timeline zoom" | ✅ | populated.png |
| Empty timeline | Primer empty states | `EmptyState` "Your slideshow is empty" with "Add all from library" | ✅ | `EditorPage` |
| Context menu | Fluent: content elements get a context menu | none | ❌ | |
| Tooltip on truncated title | Fluent tooltips: show info "not available elsewhere"; long names are elided | none | ❌ | |
| Snapping guides | Premiere/Final Cut snap to playhead and clip edges (industry convention; not in the fetched pages) | none | ❌ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Spectrum card | `surface-card`, 1 px `border` | ✅ | gallery "photo" |
| hover | Spectrum card hover; NN/G: signifier on hover | background `surface-card.brighter(0.15)` 120 ms; trim bars to 70 % | ✅ (not simulable in gallery) | manual-checks "trim handles brighten" |
| pressed / active | | press selects and moves the playhead to the clip start (immediate feedback) | ✅ | `clip_pressed` |
| selected | DESIGN §3 `selection-background` | `selection-tint` fill + 2 px `Theme.selection` border, trim bars at 70 % | ✅ | gallery "selected photo" |
| focus (keyboard) | Fluent focus visual on the focused item | none; `ClipView` has no focus, only the page-level `FocusScope` | ❌ | |
| dragging (reorder) | Apple: drag image; React Aria: preview | no change on the dragged clips; drop marker only | ❌ | |
| trim-hover | NN/G cursor: "Use the platform's standard cursor for moving or resizing" | `MouseCursor.col-resize` on the handles, bars brighten | ✅ | `trim-left`/`trim-right` |
| trimming (in progress) | NN/G: effects "immediately visible"; DESIGN §5: Escape cancels every gesture | the clip resizes under the pointer, its duration read-out updates, the playhead jumps to the new first / last frame and the preview shows it (rendered from a display copy; the project changes once, on release, as one `SetTrim`); Escape restores the original trim without an undo step | ✅ | `trim_dragged`, `cancel_trim`; manual-checks M3 |
| muted | | badge shown; audio section in inspector | ✅ | gallery "muted video" |
| disabled | | — no disabled clips | ➖ | |
| playing | Apple: Space toggles | playhead advances, play icon becomes pause, `Play (Space)` tooltip | ✅ | `tick`, transport `Bar` |
| drop target (library drag over strip) | Fluent/Apple | tinted strip + accent marker | ✅ | manual-checks M2 |
| gallery coverage | DESIGN §4 | photo / selected+dissolve / video / muted+fade / no-thumb; M4: moving / moving+slide / video+wipe / selected moving+zoom present; **ruler + playhead + drop marker + trim marker** have no gallery row | ❌ | gallery |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| clip height | Premiere track ≈ 100 px at default | 100 px (`clip-height`) | ✅ |
| min clip width | must stay clickable (DESIGN 24 px target) | 56 px (`MIN_CLIP_WIDTH`) | ✅ |
| width ↔ time | proportional to duration | `secs × pixels-per-second`, transitions pull left by the overlap (capped at half a clip) | ✅ (`layout_is_proportional_with_minimum_width`, `transitions_pull_clips_left`) |
| radius | DESIGN 6 px cards | `radius-m` | ✅ |
| type stripe | | 3 px | ✅ |
| footer height / type | DESIGN 24 px size S, 11 px caption | 22 px, 11 px | ❌ nice — 22 is not on the 24 px sizing step |
| trim handle hit area | DESIGN §2 min click target 24 × 24 | 10 × 100 px | ❌ width below 24 px |
| ruler height / ticks | DESIGN 24 px dense controls | 24 px; 8 / 4 px ticks; labels every 1/2/5/10 s by zoom | ✅ |
| playhead | | 2 px line, 12 px grab head | ✅ |
| drop marker | | 3 px accent | ✅ |
| zoom range | | 10–200 px/s, default 40 | ✅ |
| drag threshold (reorder) | Apple ≈ 3 pt; library uses 6 px | none: the first move event with the button down sets a drop marker | ❌ |
| min trimmed length | | 200 ms clamp, out ≤ natural duration | ✅ (`trim_for`) |
| tabular figures for timecodes | DESIGN §2 | not set (Slint exposes no font features) | ➖ platform limitation |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Click selects, Ctrl/Cmd toggles, Shift ranges | Fluent Extended selection; DESIGN §3 | `Selection::click` with anchor; press on a selected clip keeps the group until release | ✅ | `selection_click_semantics`; manual-checks M2 |
| Ctrl/Cmd+A selects all | Fluent shortcuts; DESIGN §3 | `FocusScope` handles `a` with meta/control; "Select all" button in inspector | ✅ | `EditorPage.keys` |
| Escape clears selection | React Aria GridList `escapeKeyBehavior` "clearSelection" by default; DESIGN §3 | none | ❌ | |
| Delete / Backspace removes selection | DESIGN §3 bulk actions, one undo step | `delete-selected` → `Command::RemoveClips` | ✅ | `keyboard.md` |
| Drag to reorder | Fluent `CanReorderItems`; Spectrum `onReorder`; NN/G centre rule | `drop_index` centre rule → `Command::move_clips` (no-op returns `None`) | ✅ | `drop_index_and_hit_testing`, `move_clips` |
| Move happens on release, preview during drag | NN/G: reflow animation ≈ 100 ms before drop | marker only; no live reflow or animation | ➖ marker is the conventional NLE preview; animation see gaps | |
| Escape cancels a drag or trim | Fluent Esc "cancel transient UI (along with any ongoing actions)" | none; `drag` is only cleared on release | ❌ | |
| Undo after move / trim | NN/G reversible; Apple undo drops | `Reorder` and `SetTrim` are commands with inverses | ✅ | core proptests |
| Trim by dragging edges | NN/G direct manipulation; inspector caption "Drag the edges of a video clip to trim it." | left/right `TouchArea`, live marker and preview frame, applied on release | ✅ | manual-checks M3 |
| Trim precision alternative | NN/G: "provide keyboard alternatives for precision tasks" | none (no numeric in/out fields, no nudge keys) | ❌ | |
| Snapping to playhead / clip edges | NLE convention (Premiere "Snap", Final Cut "Snapping") | none | ❌ | |
| Scrub by press/drag in ruler or empty strip | Apple: scrubber; Spectrum Slider | `TouchArea` under the content maps x → time (`time_at_x`), pauses playback, re-renders preview | ✅ | `time_and_pixels_round_trip` |
| Scrub over clips | | clips intercept the press (selection wins), so scrubbing works only in the ruler and empty area | ➖ press on a clip moves the playhead to its start instead | `clip_pressed` |
| Playhead grab head draggable | Apple scrubber thumb | head is a passive `Rectangle`; dragging it works because events fall through to the scrub `TouchArea` | ✅ | |
| Space plays / pauses | Apple: "people expect to press Space ... to play or pause"; `keyboard.md` | `Key.Space` → `toggle-play` | ✅ | `keys` |
| J / K / L | `keyboard.md` M3 | `j` step −1 s, `k` pause, `l` play | ✅ | `keys` |
| ← / → step frames, Shift ×10, Home to start | `keyboard.md` M3 | `step-frames(±1 / ±10)`, `Home` → `scrub(0)` | ✅ | `keys`; manual-checks M3 |
| End to the end | Fluent Home/End symmetry | none | ❌ nice | |
| Arrow keys move focus between clips | Fluent inner navigation; React Aria GridList | ← / → are bound to frame stepping; there is no clip focus at all | ❌ (design decision needed: e.g. Tab into the strip, then ←/→ focus clips, Shift+←/→ extend) | |
| Keyboard reorder | React Aria: Enter starts drag mode, arrows pick target, Enter drops; NN/G keyboard alternative | none | ❌ | |
| Double-click on clip | Spectrum highlight: double-click performs the action | nothing | ➖ no item action defined; press already jumps the playhead to the clip | |
| Zoom | Fluent "Semantic zoom Ctrl++ or Ctrl+-" | slider only; no shortcut, no Ctrl+wheel, no zoom-to-fit | ❌ nice | |
| Zoom keeps the playhead in view | NLE convention | strip relayouts, `Flickable` scroll unchanged | ❌ nice | |
| Auto-scroll while playing | Apple scrubber follows | `Flickable` is not scrolled to follow the playhead | ❌ should | `tick` |
| Context menu on right-click | Fluent context menus | none (`left` only) | ❌ | |
| Tooltip on elided title | Fluent tooltips | none | ❌ | |
| Feedback ≤ 100 ms | DESIGN §3 | selection and marker are synchronous; preview render throttled to 33 ms | ✅ | `PREVIEW_MIN_INTERVAL` |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | list item / grid cell | `accessible-role: button` on `ClipView` | ❌ nice — `list-item` and a `list` role on the strip |
| label | name + duration | `accessible-label: root.item.title` (duration and type not included) | ❌ nice — label should read "IMG_4021.jpg, photo, 4 s, muted" |
| selected state exposed | ARIA `aria-selected` | none | ❌ nice |
| trim handles reachable without a mouse | NN/G keyboard alternative | none | ❌ (same as trim precision gap) |
| playhead position announced | | transport text "00:00:07 / 00:00:26" is plain `BodyText` | ✅ readable; no live-region concept in Slint |
| contrast | DESIGN §5 | footer text `Theme.text` on `surface-card`; playhead `#f25f5c` on dark | ➖ not measured here; tokens owned by `theme.slint` |
| focus visible | Fluent | none | ❌ (see keyboard gaps) |

## Wording

| String | Status |
|---|---|
| "Muted" | ✅ German "Stumm" |
| "Timeline zoom", "Play (Space)", "Pause (Space)" | ✅ shortcut shown in tooltip (Fluent tooltips recommendation) |
| "Drag the edges of a video clip to trim it." | ✅ full sentence, ends with a period |
| "Select all", "Remove from timeline" / "Remove {} from timeline" | ✅ count shown |
| "Your slideshow is empty" + description mentioning undo | ✅ |
| Ruler labels `m:ss` | ✅ numeric, untranslated |

## Gaps

1. **[blocker] Clips are not reachable by keyboard.** No focus, no arrow navigation between clips, no keyboard reorder (Fluent inner navigation, React Aria drag mode, NN/G keyboard alternative, DESIGN §3). Fix: `focused-clip` in `EditorState`; Tab lands on the strip, then ←/→ move focus (and, with Shift, extend the selection via `Selection::click(shift=true)`), Space toggles, Enter jumps the playhead; frame stepping moves to a modifier or stays on ←/→ only while no clip is focused (decision to record in `keyboard.md`); Alt+←/→ moves the selection one slot (`Command::move_clips`). Draw the focus ring on `ClipView`. Scope: slint + view-model (`editor_view.rs` gets `move_focus`); reorder uses the existing core `Reorder`.
2. **[should] Escape neither clears the selection nor cancels a drag or trim in progress.** Fluent Esc, React Aria. Fix: `Key.Escape` in `keys`: if `drag`/trim active → drop the state, clear markers, `sync_timeline`; else `Selection::clear`. Scope: slint + view-model.
3. **[should] No drag threshold and no drag visual for clip reordering.** Apple ≈ 3 pt; Fluent/React Aria drag preview. A jittery click already shows a drop marker; the moved clips give no visual cue. Fix: reuse `DRAG_THRESHOLD` (move it to a shared module) in `clip_dragged`; set `opacity: 0.6` on selected clips while `drag` is active and show the library ghost pill ("{n} clips") or a translucent copy. Scope: view-model + slint.
4. **[should] No snapping.** NLE convention; makes precise placement of the playhead and trim edges hard with direct manipulation alone (NN/G precision drawback). Fix: `editor_view::snap(x, candidates, tolerance_px)` for playhead-to-clip-edges and trim-edge-to-playhead, Shift to bypass, a highlighted guide line while snapped. Scope: view-model (pure function, tests) + slint guide.
5. **[should] Trim has no precision alternative.** NN/G "keyboard alternatives for precision tasks". Fix: `,` / `.` (or Alt+←/→) nudge the focused clip's in/out point by one frame, and numeric in/out fields in the inspector Audio/Video section, all through `Command::SetTrim`. Scope: slint + view-model.
6. **[should] Strip does not follow the playhead while playing** and zoom does not keep the playhead in view. Fix: in `tick`/`zoom-changed` set `flick.viewport-x` so the playhead stays inside the viewport (page-scroll when it leaves the right edge). Scope: slint + view-model (expose `strip-scroll-to`).
7. **[should] No context menu on clips.** Fluent. Fix: right button in `ClipView.pointer-event` → `ContextMenuArea` with "Remove {n} from timeline", "Cut / Cross dissolve / Fade through black", "Mute", "Rotate 90°", "Show in library"; Shift+F10 once focus exists. Scope: slint + view-model (callbacks already exist).
8. **[should] No gallery row for ruler, playhead, drop marker and trim marker.** DESIGN §4 requires every state in the gallery. Fix: a "Timeline strip" row rendering a 600 px strip with two clips, ruler at 40 px/s, playhead, drop marker and trim marker. Scope: slint (gallery) only.
9. **[nice] Trim handle hit width is 10 px** (DESIGN 24 px minimum target). Fix: widen the `TouchArea` to 16 px (24 would eat too much of a 56 px clip) and keep the 4 px bar. Scope: slint only.
10. **[nice] Footer is 22 px**, off the 24 px step. Fix: `Theme.control-height-s`. Scope: slint only.
11. **[nice] No tooltip on elided titles.** Fluent tooltips. Fix: `Tooltip { text: item.title }` on `ClipView` when the label `Text` reports overflow (Slint lacks an overflow signal: show when `title.length > 18` at the current width, or always for clips narrower than 120 px). Scope: slint only.
12. **[nice] Zoom has no shortcut and no zoom-to-fit.** Fluent Ctrl++ / Ctrl+-. Fix: bind `=`/`-` with meta/control to ±20 px/s and `Shift+Z` to fit; document in `keyboard.md`. Scope: slint + view-model.
13. **[nice] `End` key** to jump to the end (Fluent Home/End pair). Fix: `Key.End` → `scrub(strip-width)`. Scope: slint only.
14. **[nice] Accessibility roles and labels.** `ClipView` as `list-item` with selected state and a label including type, duration and muted; strip as `list`. Scope: slint only.
15. **[nice] Drop marker colour documented as "orange"** in `docs/ux/manual-checks.md` while the code uses `Theme.accent`. Fix: update the manual check text. Scope: docs.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: Escape clears the selection, End jumps to the end; trim hit area 16 px; footer on the 24 px step; placeholder icon on clips without thumbnail; `TimelineStrip` extracted and shown in the gallery (ruler, playhead, drop marker); manual-checks wording.
- Deferred to AUDIT-2026-09: keyboard access #2, context menu #5, snapping #6, precise trim #7, follow playhead #8, drag visual #9, zoom shortcuts #16, a11y #18.

## Review 2026-09-25 (M4), open for triage

- **nice** — on videos without a transition the badge's left 8 px sit under
  the 16 px trim handle, so its tooltip only appears over the right part.
- **nice** — on minimum-width clips (56 px) with a long overlap the badge is
  clipped by the card.
- **nice** — the motion badge tooltip says "Moves (Ken Burns)"; "Ken Burns"
  is jargon (NN/G "match with the real world"). Alternative: "Slow zoom or pan".
- Keyboard: `T` focuses the transition picker in the inspector
  (`keyboard.md`); like J/K/L it is not shown in a menu yet.

## Resolution 2026-09-25

- Fixed: keyboard access (AUDIT #2): ↑/↓ previous/next clip with playhead jump (←/→ keep frame stepping), Shift extends, Ctrl/Cmd moves focus only, Enter toggles, Alt/Option+←/→ reorders by one place (one `Reorder` per press); the strip is its own tab stop, clicks focus it, the focused clip scrolls into view. Inset keyboard-only focus ring. Clips are `list-item`s with selected state; labels include duration and muted. Gallery rows for selected+focus and focus only.
