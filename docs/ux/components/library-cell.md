# Library cell and grid (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/library.slint` → `CellView`, `KindBadge`, grid section of `LibraryPage` (ListView of `GridRow`, marquee TouchArea, marquee rectangle); selection, drag and layout maths in `crates/app/src/library_view.rs` (`GridSelection`, `cells_in_rect`, `DRAG_THRESHOLD`) and `crates/app/src/library_ui.rs` (`cell_pressed` / `cell_drag_moved` / `cell_released` / `marquee_*`); drag ghost in `crates/app/ui/main.slint`.
Gallery rows: "Library cells: photo, selected, video, audio (no thumb), pending, failed, cloud" and "Badges: video, audio, cloud, failed, muted; HDR tag" in `crates/app/ui/gallery.slint`.

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/listview-and-gridview (the `/design/controls/list-view-and-grid-view` URL now 404s and redirects here) | selection modes (Single / Multiple / Extended), click vs. select, drag and drop between grids |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/item-templates-gridview | item template: image + text below, caption style for subtitle, accessible name on the template root |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/input/drag-and-drop | drag UI (glyph, caption, content), DragOver feedback, press-and-hold vs. drag disambiguation |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/input/keyboard-interactions | arrow-key inner navigation of grids, Home/End/PageUp, Space/Enter, Esc, Shift+Arrow range, Ctrl+A, focus visual |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/menus-and-context-menus | context menu on elements whose primary purpose is content |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/tooltips | tooltip only for information not visible elsewhere |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ListView.html | highlight selection style, single click selects / double click acts, drop positions before/after/on/root, loading state, aria-label |
| Spectrum (React Aria) | https://react-aria.adobe.com/GridList and https://react-aria.adobe.com/dnd (redirected from `react-spectrum.adobe.com/react-aria/...`) | Escape clears selection, Shift+Arrow, typeahead, keyboard drag and drop, drag preview with count badge, drop indicators |
| NN/G | https://www.nngroup.com/articles/drag-drop/ | cursor and ghost signifiers, centre-overlap threshold, ~100 ms reflow animation, keyboard alternative |
| NN/G | https://www.nngroup.com/videos/bulk-actions-design-guidelines/ | select all, contextual action bar, feedback with undo (page summary only; the video is not transcribed) |
| Apple HIG | https://developer.apple.com/design/human-interface-guidelines/drag-and-drop (read via the `tutorials/data/.../drag-and-drop.json` payload; the HTML page is JS-rendered) | drag image after ~3 pt, translucent ghost, count badge, feedback for failed drops, undo of drops |

Spectrum "Card" (`react-spectrum.adobe.com/react-spectrum/Card.html`) returns 404 and could not be used.

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Item container | Fluent template: fixed-size `StackPanel` per item, image on top, text below | `CellView` 104 × 112, `radius-m` (6 px), padding 4, `Theme.selection-tint` / `hover-tint` background | ✅ | gallery "Library cells" |
| Thumbnail | Fluent: `Image` `Stretch="UniformToFill"`; Spectrum: preview area | `Image` 78 px tall, `image-fit: contain` on `surface-card`, `radius-s` | ✅ (contain instead of fill is deliberate so portrait photos are not cropped in the catalogue) | gallery "Library cells: photo" |
| Placeholder while loading | Spectrum ListView: "progress circle reflecting the current load state" | Kind icon 28 px in `text-disabled` while `pending` | ✅ (static icon, no spinner; see states) | gallery "pending" |
| Type badge | Spectrum Badge; Fluent icon glyph | `KindBadge` top-right, 4 px inset: video (purple), audio (green); photos carry no badge | ✅ | gallery "Badges", "video", "audio" |
| Status badge (cloud / failed) | Primer degraded experiences: show status inline | Same `KindBadge`: cloud icon in `text`, error icon in `danger`; failed wins over cloud wins over kind | ✅ | gallery "failed", "cloud" |
| Title | Fluent: `TextBlock` below image, `AutomationProperties.Name` bound to it | `Text` 11 px `Theme.text`, `overflow: elide`, centred | ✅ | gallery row |
| Subtitle | Fluent: `CaptionTextBlockStyle`, `SystemControlPageTextBaseMediumBrush` | `Text` 11 px `text-secondary`, `elide`; duration for video/audio, dimensions for photos, empty when unknown | ✅ | gallery "0:14 · 4K", "3:42" |
| Selection checkbox | Fluent GridView Multiple mode shows checkboxes; Spectrum `selectionStyle="highlight"` hides them | — highlight style chosen (accent ring + tint); checkboxes are a touch pattern | ➖ | |
| Drag handle | NN/G: grab handles are "not nearly as universal as designers may think"; React Aria: explicit drag button for keyboard users | — whole cell is the drag source; no handle | ➖ (NN/G) — but see keyboard gap 8 | |
| Marquee rectangle | Fluent/Explorer rubber band | `Rectangle` `selection-tint` fill, 1 px `selection` border, positioned by Rust in grid coordinates | ✅ | manual-checks M2 "rubber band" |
| Drag ghost | Fluent DragUI: glyph + caption + content; Apple: translucent image + count badge; NN/G: "translucent ghost preview" | Pill in `main.slint`: `image-multiple` icon + "{n} items", accent when droppable, drop shadow | ❌ no content preview (thumbnail) in the ghost | manual-checks "Design guide pass" |
| Empty grid | Primer empty states | `EmptyState` "Your library is empty" / "Nothing matches your search" | ✅ | gallery "EmptyState" |
| Context menu | Fluent: "If the host element ... has another primary purpose (such as presenting text or an image), use a context menu" | none; actions live in the collapsible details pane and the bottom `Bar` | ❌ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Fluent quiet item | transparent background over `surface-window` | ✅ | gallery "photo" |
| hover | Fluent list item hover fill; Spectrum quiet card hover | `hover-tint` (foreground 8 %) animated 120 ms; badge and text unchanged | ✅ (not simulable in gallery) | manual-checks "hover tint" |
| pressed / active | Fluent pressed fill | none; press only starts selection/drag | ➖ press is consumed by selection change (<100 ms feedback via ring) | |
| selected | Fluent selection border; Spectrum highlight background; DESIGN §3 `Palette.selection-background` | `selection-tint` fill + 2 px `Theme.selection` ring | ✅ | gallery "selected" |
| focus (keyboard) | Fluent: "rendered as a highlighted border around the UI element"; keyboard-interactions: arrow keys move focus inside GridView | none; `CellView` has no `FocusScope`, the grid is not a tab stop | ❌ | |
| disabled | Spectrum `disabledKeys` | — no disabled cells; cloud/failed items stay selectable so they can be removed | ➖ | |
| pending thumbnail | Spectrum: progress circle; Primer loading: placeholder | kind icon at `text-disabled`; `pending` cleared on `ThumbReady`/`ThumbFailed` | ✅ (deliberately no spinner per cell: hundreds may load at once) | gallery "pending" |
| failed | Primer degraded: say what happened | error badge with label "Could not be read"; details pane shows the message | ✅ | gallery "failed" |
| cloud placeholder | Primer degraded | cloud badge "Not downloaded"; no download triggered | ✅ | gallery "cloud"; manual-checks M1 |
| no thumbnail (audio) | Fluent icon template | music-note icon, audio badge | ✅ | gallery "audio (no thumb)" |
| dragging (source) | Apple: drag image "as soon as people drag ... about three points"; Fluent: content visible in DragUI | source cells unchanged; ghost pill follows pointer after 6 px | ❌ source not dimmed, no thumbnail in ghost (see anatomy) | |
| drop over target | Fluent DragOver: target changes DragUI; Apple: highlight only while above destination | ghost turns accent, timeline strip tints `drop-target` with accent border, insertion marker | ✅ | manual-checks M2 |
| marquee in progress | Explorer: live selection | cells update live as the band moves (`marquee_move`) | ✅ | `grid_selection_semantics`, `cell_rects_and_marquee_hits` tests |
| gallery coverage | DESIGN §4: every state has a gallery row | photo / selected / video / audio / pending / failed / cloud present; **focus, dragging, marquee** absent | ❌ (focus row missing once focus exists; marquee/drag are gestures, ➖) | gallery |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| cell size | Fluent template 180 × 280 (image 180 + text); any fixed size acceptable | 104 × 112 (thumb 78 + 2 × 11 px text + padding), mirrored as `CELL_VISIBLE_*` in `library_view.rs` | ✅ |
| gap between cells | Fluent `Margin="12"`; DESIGN §2: related controls 8 | 8 px (`space-s`) horizontal, 4 px vertical (row 116 − 112) | ❌ vertical gap 4 px is not the 8 px used horizontally; cells read as denser vertically than horizontally |
| grid padding | Fluent `Margin="40,0"` on the panel; DESIGN §2 panel padding 16 | 12 px left (`space-m`, `GRID_PADDING`), 0 right | ❌ should be symmetric (right padding missing; last column touches the scrollbar) |
| radius | DESIGN §2: 6 px cards | `radius-m` (6) cell, `radius-s` (4) thumbnail | ✅ |
| badge size / inset | Spectrum Badge S | `Badge` component, 4 px inset | ✅ |
| type size | Fluent caption for subtitle, body for title | 11 px for both title and subtitle | ➖ 11 px title is a deliberate density choice (DESIGN scale allows 11 caption); differs from Fluent body title |
| selection ring | Fluent 2 px accent border | 2 px `Theme.selection` | ✅ |
| drag threshold | Apple ≈ 3 pt; Fluent unspecified | 6 px (`DRAG_THRESHOLD`) | ✅ |
| min click target | DESIGN §2: 24 × 24 | 104 × 112 | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Click selects one, replaces selection | Fluent Extended "same as Single"; Spectrum highlight: "Clicking a row ... replaces the selection with only that row" | `GridSelection::click(shift=false, toggle=false)`; press on an already-selected cell keeps the group until release (so it can be dragged), then collapses | ✅ | `grid_selection_semantics`; manual-checks M2 |
| Ctrl/Cmd-click toggles | Fluent Extended Ctrl: "select multiple items ... to select the focused item"; DESIGN §3 | `toggle` from `e.modifiers.control || e.modifiers.meta` | ✅ | `grid_selection_semantics` |
| Shift-click ranges from anchor | Fluent: "select multiple contiguous items by clicking ... the first item ... and then ... the last item" | `anchor` kept; Shift+Ctrl adds the range without clearing | ✅ | `grid_selection_semantics` ("shift+toggle adds a range") |
| Marquee on empty space | Explorer/Finder rubber band; DESIGN §3 | `marquee-start` on the row TouchArea; Shift/Ctrl at start keeps `base` selection (additive) | ✅ | `cell_rects_and_marquee_hits`; manual-checks M2 |
| Ctrl/Cmd+A selects all | Fluent common shortcuts "Select all Ctrl+A"; NN/G bulk actions "provide a Select All option"; DESIGN §3 | none in the library (only the editor `FocusScope` handles Ctrl+A for clips); no "Select all" button either | ❌ | |
| Escape clears selection | React Aria GridList: "Escape key: Clears selection by default"; DESIGN §3 | none | ❌ | |
| Arrow keys move focus / selection in the grid | Fluent keyboard-interactions: "If items are multiple columns, all 4 arrow keys navigate"; row-major wrap; Shift+Arrow "Continuously select" | none; no key handling in `library.slint` | ❌ | |
| Home / End / PageUp / PageDown | Fluent: Home/End move focus to first/last item and scroll it into view; Page keys scroll a viewport | none | ❌ | |
| Space / Enter | Fluent: Space selects focused item; Enter performs the additional action if one exists | none (no focus) | ❌ | |
| Double-click performs the item action | Spectrum highlight: "Single clicking selects the row and actions are performed via double click" | `double-clicked` → `cell-double-clicked` → add that item to the timeline | ✅ | manual-checks M2 "Double-click adds one" |
| Drag selected cells | Fluent GridView `CanDragItems`; Apple: "select and drag content with a single motion" | press on a selected cell drags the whole selection; press on an unselected cell selects then drags it | ✅ | `cell_pressed` / `cell_drag_moved` |
| Drop insertion follows the centre rule | NN/G: reshuffle "once the center of the dragged object overlaps the edge of the other object" | `drop_index`: before the clip whose centre is right of x | ✅ | `drop_index_and_hit_testing` |
| Escape cancels a drag | Fluent Esc "cancel transient UI (along with any ongoing actions)"; Apple: cancellation feedback | none; only releasing outside the strip abandons the drag, silently | ❌ | |
| Failed drop feedback | Apple: item "move[s] back ... or scale[s] up and fade[s] out" | ghost disappears instantly, nothing else | ❌ nice | |
| Undo after drop | Apple: "Prefer letting people undo a drag-and-drop operation"; NN/G bulk: "option to undo" | insertion is `Command::InsertClips`, one undo step | ✅ | editor undo tests |
| Bulk action shows count | NN/G bulk actions: contextual action bar; DESIGN §3 "Remove 12 clips" | "Add {} to timeline" in the bottom `Bar`, status bar "{} selected" | ✅ | `library.slint` action bar |
| Bulk remove is undoable or confirmed | DESIGN §3: destructive + not undoable → confirm | "Remove from library" removes `n` items with neither undo nor confirmation (library op, not a `Command`) | ❌ | `remove_selected` |
| Context menu on right-click | Fluent context menus: "invoked by right clicking"; Shift+F10 / Menu key | right button ignored (`PointerEventButton.left` only) | ❌ | |
| Press-and-hold vs. drag (touch) | Fluent: drag within 500 ms drags, else context menu | — not a touch target | ➖ | |
| Selection survives refresh | Fluent SelectedItems synchronised with items | `retain_existing` after every requery | ✅ | `grid_selection_semantics` |
| Keyboard alternative to drag | NN/G: handle "keyboard accessible with the Tab key ... 'grabbing' via the spacebar"; React Aria: Enter starts drag mode | "Add to timeline" button and double-click cover the outcome; no keyboard path because the grid has no focus | ❌ (covered only once arrow navigation + Enter exist) | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | Fluent GridView items are list items; Spectrum ListView rows | `accessible-role: button` on `CellView` | ❌ nice — should be `list-item` (Slint has it); the grid itself has no `accessible-role: list` |
| label | Fluent: "set the AutomationProperties.Name on the root element of the DataTemplate" | `accessible-label: root.cell.title` | ✅ |
| selected state exposed | Spectrum/ARIA `aria-selected` | not exposed (no `accessible-checked` / selected) | ❌ nice |
| badge meaning not icon-only | DESIGN §2 icons never carry meaning alone | `KindBadge.label` "Video" / "Audio" / "Not downloaded" / "Could not be read" | ✅ |
| contrast (text 4.5:1, UI 3:1) | WCAG via DESIGN §5 | `text-secondary` is foreground at 62 % alpha over `surface-window`; badges use `badge-backdrop` 70 % black | ➖ not measured in this audit; tokens owned by `theme.slint` |
| focus visible | Fluent focus visual | none (no focus) | ❌ (same as keyboard gap) |
| list aria-label | Spectrum: "An aria-label must be provided to the ListView" | none on the `ListView` | ❌ nice |

## Wording

Primer content rules: sentence case, verb-first, ellipsis on dialog openers.

| String | Status |
|---|---|
| "Video", "Audio", "Not downloaded", "Could not be read" (badge labels) | ✅ sentence case, German present |
| "Add to timeline" / "Add {} to timeline", "Add all" | ✅ verb-first, count shown |
| "Add files…", "Add folder…" | ✅ ellipsis (open a dialog) |
| "Drop to add to your library" | ✅ |
| "1 item" / "{} items" (ghost, header) | ✅ |
| "Remove from library", "Show in folder" | ✅ |

## Gaps

1. **[blocker] No keyboard access to the grid.** No focus, no arrow navigation, no Space/Enter, no Home/End. Fluent keyboard-interactions and DESIGN §3 ("every action reachable by mouse is reachable by keyboard"). Fix: give the grid a `FocusScope` (single tab stop) with a `focused-index` in `LibraryState`; arrows move focus row-major (`columns` is already known in `library_ui.rs`), Shift+Arrow extends from the anchor via `GridSelection::click(shift=true)`, Space toggles, Enter adds to timeline, Home/End/PageUp/PageDown scroll; draw a `focus-ring` on the focused `CellView`; add a "focused" gallery cell. Scope: slint + view-model (`library_view.rs` gets `move_focus(dir, columns, items)` with tests).
2. **[should] No Ctrl/Cmd+A and no Escape in the library.** React Aria GridList, Fluent shortcuts, DESIGN §3. Fix: handle in the same `FocusScope`; add `select_all` / `clear` callbacks (`GridSelection` already has `clear`). Scope: slint + view-model.
3. **[should] No context menu on cells.** Fluent menus-and-context-menus. Fix: right button in `CellView.pointer-event` opens a `ContextMenuArea` with "Add to timeline", "Show in folder", "Remove from library" acting on the selection (right-click on an unselected cell selects it first, Explorer style); Shift+F10 once focus exists. Scope: slint + view-model.
4. **[should] Bulk "Remove from library" has neither undo nor confirmation.** DESIGN §3 and NN/G bulk actions. Fix: either make removal undoable (library-level restore; not a core `Command` because the catalogue is not the project) or confirm with the count ("Remove 12 items from the library?"). Scope: view-model + `library` crate API (`Library::restore`) or slint dialog.
5. **[should] Escape does not cancel a drag.** Fluent Esc semantics, Apple cancellation. Fix: key handler while `press.dragging` sets `Shell.drag-active = false`, clears the drop marker and drops `press`. Scope: view-model + slint (needs the `FocusScope` from gap 1 or a window-level key handler).
6. **[should] Grid padding is asymmetric and the vertical gap is 4 px.** DESIGN §2 scale says related controls 8 px; right padding is 0. Fix: `padding-right: Theme.space-m` on the row `HorizontalLayout`, row height 120 (or cell 108) so the vertical gap is 8; update `ROW_HEIGHT` / `columns_for_width` and their tests. Scope: slint + view-model constants.
7. **[nice] Drag ghost shows no content preview and source cells are not dimmed.** Apple "translucent representation of the content", Fluent `IsContentVisible`, NN/G ghost preview. Fix: add `Shell.drag-image` (first selected thumbnail) to the ghost pill; set `opacity: 0.6` on `CellView` while `Shell.drag-active && cell.selected`. Scope: slint + view-model (pass the image).
8. **[nice] No keyboard alternative to drag-and-drop into the timeline.** NN/G, React Aria drag button. Fix: Enter on a focused cell = add to timeline at the playhead (falls out of gap 1); document in `docs/ux/keyboard.md`. Scope: slint + view-model.
9. **[nice] Failed drop gives no feedback.** Apple HIG. Fix: 120 ms fade-out of the ghost at the release point instead of instant removal. Scope: slint only.
10. **[nice] Accessibility roles.** `CellView` should be `list-item` with the selected state exposed, the `ListView` should carry `accessible-role: list` and a label ("Library"). Scope: slint only.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: Ctrl/Cmd+A selects all, Escape clears or cancels a drag (library `FocusScope`, focused on click); grid padding symmetric and rows 120 px (8 px gap); placeholder icon token.
- Deferred to AUDIT-2026-09: keyboard navigation #1, remove undo #3, context menu #4, drag ghost #14, a11y roles #18.

## Resolution 2026-09-25

- Fixed: keyboard access (AUDIT #1): the grid is one tab stop with Fluent extended-selection keys (arrows, Shift, Ctrl/Cmd, Space, Home/End, Page Up/Down, Enter adds to timeline), keyboard-only focus ring outside the selection border, focused row scrolls into view, clicks move the keyboard focus. Cells are `list-item`s with selected state inside a labelled `list`. Gallery rows for selected+focus and focus only. Decision note: `docs/ux/decisions/keyboard-navigation.md`.
