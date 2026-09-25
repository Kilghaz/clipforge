# TextLane (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/editor.slint` → `TextLane` (inside `TimelineStrip`, above the clips)
Gallery rows: "TextLane (28 px rows): texts, a selected one, overlaps stacked into a second row; empty lane with the double-click hint"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| NN/G | https://www.nngroup.com/articles/direct-manipulation/ | move and trim in place with continuous feedback; reversible |
| Fluent 2 | keyboard interactions (M3 card) | Delete removes, Escape cancels a drag, Shift / Ctrl-click extend the selection |
| Premiere / Clipchamp | text track above the video track (convention) | texts as timeline items with their own start and length |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Track | track above video | `surface-pane` lane, `radius-m`, 1 px border, 28 px per row, up to 3 rows | ✅ | gallery "TextLane" |
| Text block | item on the time scale | x from `x_at_time` (same scale as clips), min 24 px, `Theme.text-track` at 45 % (60 % hover, 85 % selected) | ✅ | `text_blocks_follow_the_strip_scale` |
| Label | first line + type icon | `text_t` icon 16 px + caption 11 px, elided | ✅ | gallery |
| Overlap rows | — (ours) | greedy rows so no text hides another | ✅ | `text_rows_stack_overlapping_items` |
| Trim edges | edge handles like clip trim | 8 px hit areas, 3 px bar shown on hover / selection | ✅ | gallery row 1 |
| Empty hint | Primer empty states | "Double-click to add text" with the icon | ✅ | gallery row 2 |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default / hover / selected | selection ring (DESIGN §3) | tint steps + 2 px `selection` border | ✅ | gallery |
| dragging | live | block and preview follow; committed on release | ✅ | manual-checks M5 |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| click selects, Shift/Ctrl/Cmd-click extends | DESIGN §3 | `text-lane-pressed` → `select_text` | ✅ | manual-checks M5 |
| drag moves in time, edges trim (≥ 0.2 s), one undo step | NN/G | `lane_drag` + `SetTexts` on release | ✅ | `lane_drag_moves_and_trims_with_a_minimum_length` |
| double-click empty space adds a 4 s text there | efficiency | `text-lane-double-clicked` | ✅ | manual-checks M5 |
| double-click a block edits it in place | | `preview-text-double-clicked` | ✅ | |
| Delete removes, Escape cancels a drag | Fluent | `delete_selected`, `cancel_text_drag` | ✅ | keyboard.md |
| Tab reaches the lane; ←/→ move the focus between texts in time order (playhead follows); Enter / Space selects, Enter again edits in place | DESIGN §3 (everything by keyboard), Fluent list keyboard | `text-focus` FocusScope, `text_nav`, `text_lane_activate`; 2 px `focus-ring` on the focused block | ✅ | `keyboard_moves_between_texts_in_time_order`, gallery focus row |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | list / list items, selectable | `list`, `list-item` with selected state | ✅ |
| label | the text | first line | ✅ |

## Gaps

None open (keyboard access added 2026-09-25).
