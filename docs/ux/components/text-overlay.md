# TextOverlay (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/editor.slint` → `TextOverlay` (over the preview image)
Gallery rows: "TextOverlay over a 16:9 picture: …"; scenes `text.png`, `text-edit.png`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| NN/G | https://www.nngroup.com/articles/direct-manipulation/ | continuous representation while dragging; visible handles large enough to hit; keyboard alternatives; undo |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionGroup.html | formatting controls live in the inspector next to the canvas |
| Keynote / Canva | on-canvas text boxes (convention) | selection box with corner and side handles, double-click to type, snap guides |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Hit area per visible text | the object itself is the target | box of the laid-out text (from `TextRenderer::hit_box`), mapped into the contained picture | ✅ | `text.png` (box matches the rendered text) |
| Hover outline | affordance | 1 px `text` at 60 % | ✅ | gallery |
| Selection box | outline | 1 px `selection` | ✅ | gallery, `text.png` |
| Corner handles | resize | 10 px white squares (18 px grab area), `selection` border; scale font size and width. Fixed elements, not a repeater: a repeater was rebuilt as the box changed and dropped the handle being dragged (fixed 2026-09-25) | ✅ | `corner_handle_scales_size_and_width`, `a_corner_handle_resizes_even_while_the_box_changes` |
| Side handles | resize one dimension | left / right change the wrap width around the centre | ✅ | `side_handle_changes_width_around_the_centre` |
| Snap guides | alignment feedback | the first dragged text snaps its centre to the frame centre or another visible text's centre, and its box edges to the 5 % safe margin; 1 px `Theme.guide` lines show where | ✅ | `snapping_reaches_other_texts_and_the_safe_margin`, gallery |
| Inline editor | type in place | `TextInput` with the text's font (bundled fonts imported into Slint), size, weight, italic, colour and alignment, over the text's box colour or an offset shadow copy; the rendered copy is hidden meanwhile | ✅ | `text-edit.png` |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| idle / hover / selected / dragging / editing | | as above; dragging re-renders through the real compositor | ✅ | scenes |
| empty picture press | deselect | clears the text selection | ✅ | manual-checks M5 |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| drag moves, one undo step on release; Escape puts it back | NN/G, Fluent | `preview_text_dragged`, `text_released`, `cancel_text_drag` | ✅ | manual-checks M5 |
| double-click / Enter edits in place; Escape / click outside ends | convention | `begin_inline_edit`, `end_inline_edit` | ✅ | keyboard.md |
| arrows nudge 0.5 % (Shift 5 %) | NN/G keyboard alternative | `text_nudge` | ✅ | keyboard.md |
| typing session = one undo step | DESIGN §3 | `History::apply_merging` | ✅ | `merged_text_edits_undo_in_one_step` |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role / label | | each text a `button` "Text on the preview"; the inline field labelled "Text" | ✅ |
| keyboard path without a pointer | | Ctrl/Cmd+T adds, the inspector edits every property, arrows move | ✅ |

## Gaps

None open (shadow / box while editing and snapping to texts and the safe
margin added 2026-09-25; the editing shadow is a sharp offset copy, the
rendered one is soft).
