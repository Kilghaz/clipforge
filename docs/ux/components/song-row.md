# SongRow (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/components.slint` → `SongRow` (Music inspector)
Gallery rows: "SongRow: first, middle, last (up/down disabled at the ends), long name; …"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 | https://learn.microsoft.com/en-us/windows/apps/design/controls/listview-and-gridview | vertical list for text items read top to bottom; item actions beside the item; reorder |
| NN/G | https://www.nngroup.com/articles/ui-copy/ | icon actions carry tooltips |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionButton.html | quiet action buttons for secondary item actions |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| leading icon | list item: optional | music note 16 px, `Theme.audio` | ✅ | gallery |
| primary text | list item title | song file name, 14 px, elided | ✅ | gallery |
| secondary text | optional second line | length `m:ss`, 11 px, secondary colour | ✅ | gallery |
| item actions | beside the item | quiet `IconButton`s: move up, move down, remove | ✅ | gallery |
| drag handle / drag reorder | Fluent: drag reorder supported | — up/down buttons instead (keyboard-accessible, playlists are short) | ➖ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | | | ✅ | gallery |
| first / last | actions that cannot apply are disabled | up disabled on the first row, down on the last; quiet disabled buttons stay borderless | ✅ | gallery rows 1 and 3 |
| hover / pressed / focus | from `IconButton` | `IconButton` states | ✅ | gallery "IconButton" |
| selected | list selection | — rows are not selectable (actions only) | ➖ | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | two-line list item ≥ 40 px | 40 px (`bar-height`) | ✅ |
| buttons | 32 px controls | 32 px, 4 px apart | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| each action one undo step | DESIGN §3 | `song_move` / `song_remove` → `SetMusic` | ✅ | manual-checks M5 |
| Tab reaches every action; Enter/Space activates | Fluent button | `IconButton` | ✅ | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | list item | `accessible-role: list-item`, label "name, length" | ✅ |
| icon buttons named | tooltip + label | "Move up", "Move down", "Remove song" | ✅ |

## Wording

"Move up" / "Nach oben", "Move down" / "Nach unten", "Remove song" /
"Musikstück entfernen".

## Gaps

None open.
