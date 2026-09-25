# Keyboard access to the library grid and the timeline

AUDIT-2026-09 #1 and #2 (both blockers). Written before coding
(`/ux-prepare`), 2026-09-25.

## Task

"A user who prefers the keyboard, or cannot use a mouse, wants to pick
photos in the library, put them on the timeline, and rearrange and remove
clips there, without touching the mouse."

## Sources read

- Fluent, List view and grid view: one tab stop per collection; arrow keys
  move focus inside; in extended selection, arrows move focus *and*
  selection, Shift extends from the anchor, Ctrl moves focus only,
  Ctrl+Space toggles; Home/End and Page Up/Down.
  https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/listview-and-gridview
- Fluent, Keyboard interactions: focus visuals only for keyboard users,
  inner navigation with arrows, Tab leaves the collection.
  https://learn.microsoft.com/en-us/windows/apps/design/input/keyboard-interactions
- Spectrum / React Aria GridList: selection follows focus in "replace"
  behaviour, Space toggles, Enter performs the item's action.
  https://react-spectrum.adobe.com/react-spectrum/ListView.html
- NN/G, Direct manipulation: every drag needs a non-drag alternative.
  https://www.nngroup.com/articles/direct-manipulation/
- Editor conventions (Premiere, Final Cut): ←/→ step frames regardless of
  which panel has focus; ↑/↓ jump to the previous/next edit point.

## Decisions

### Library grid (Fluent GridView, extended selection)

| Key | Action |
|---|---|
| Tab | Enters the grid as one stop; focus goes to the last focused item, else the first selected, else the first item. Selection does not change on entry. |
| ← / → | Previous / next item in reading order (wraps across rows) |
| ↑ / ↓ | Same column, one row up / down; ↓ into a shorter last row lands on the last item |
| Home / End | First / last item |
| Page Up / Page Down | By the number of fully visible rows |
| plain arrow | Focus moves, selection follows (replaces, anchor = new item) |
| Shift + arrow | Selection = range from anchor to the new focus |
| Ctrl/Cmd + arrow | Focus moves, selection unchanged |
| Space, Ctrl/Cmd+Space | Toggle the focused item (anchor = it) |
| Enter | Add the selection to the timeline (the same as the action bar button; double-click adds one item) |
| Ctrl/Cmd+A, Esc | Unchanged: select all, clear selection or cancel drag |

A click sets the focused item silently, so the keyboard continues from where
the mouse was. The focused row scrolls into view.

### Timeline

←/→ keep stepping frames everywhere in the editor. Taking them away when the
strip has focus would break the editor convention right after a user clicks
a clip. Clip navigation uses the vertical arrows instead, the way Premiere
and Final Cut jump between edit points:

| Key | Action |
|---|---|
| Tab | The strip is its own tab stop; focus goes to the last focused clip, else the first selected, else the clip under the playhead |
| ↑ / ↓ | Previous / next clip: selects it (anchor = it) and moves the playhead to its start |
| Shift + ↑ / ↓ | Extends the selection from the anchor |
| Ctrl/Cmd + ↑ / ↓ | Moves focus only (playhead follows, selection unchanged) |
| Enter | Toggles the focused clip in the selection |
| Alt/Option + ← / → | Moves the selected clips one position earlier / later: one `Reorder` command, one undo step per press; a scattered selection is gathered into a block |
| Delete, Cmd/Ctrl+A, Esc, Space, J/K/L, Home/End | Unchanged |

Clicking a clip focuses the strip. The strip scrolls to keep the focused
clip visible.

Ctrl/Cmd+Space was rejected for the timeline toggle because Cmd+Space opens
Spotlight and Ctrl+Space switches the input source on macOS.

### Focus visual (both)

- The focused item gets a 2 px `Theme.focus-ring` outline, inset 2 px
  outside the selection border, **only while the collection has keyboard
  focus and the last focus change came from the keyboard** (Tab or an
  arrow key). A pointer click hides it; the next key press shows it again.
- Accessibility: the grid and the strip are `list`s with a label; cells and
  clips are `list-item`s with their selected state.

### Undo and feedback

- Selection and focus changes are not undo steps (as today).
- Reordering by keyboard is one `Reorder` command per press, via
  `Command::move_clips`, so undo restores the previous order.
- Feedback within 100 ms: selection tint, focus ring, playhead jump.

### Wording

No new visible strings. The keyboard map (`docs/ux/keyboard.md`) and manual
checks get the new keys.

## Tests first

`library_view.rs`:
- `grid_nav_moves_in_reading_order_and_clamps`
- `grid_nav_down_into_short_last_row_lands_on_last_item`
- `grid_nav_page_moves_by_visible_rows`
- `keyboard_select_plain_replaces_and_sets_anchor`
- `keyboard_select_shift_extends_from_anchor`
- `keyboard_toggle_flips_one_and_moves_anchor`

`editor_view.rs`:
- `nudge_left_and_right_move_the_block_by_one`
- `nudge_at_the_edges_is_none`
- `nudge_gathers_a_scattered_selection`
- `clip_nav_clamps_at_both_ends`
