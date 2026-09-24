# Bar (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `Bar`
Gallery rows: "Bar (48 px) with mixed controls: everything must sit on one centre line" in `crates/app/ui/gallery.slint`
Used in: `library.slint` (header bar, search row 40 px, filter row 48 px, selection-action bar), `editor.slint` (editor toolbar, transport bar 40 px)

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/command-bar | anatomy (content area left, primary commands right, "see more" overflow, separators), 20 px icons in the bar, short single-word labels, dynamic overflow, consistent command placement, Accept left of Cancel |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionBar.html | selection-driven action bar: selected count, clear-selection button, action group, overflow menu, Escape clears selection, hidden at 0 selected, labels collapse when space is limited |
| Apple HIG | https://developer.apple.com/design/human-interface-guidelines/toolbars | **not read**: the page is JS-rendered and WebFetch returned only the title (both the current URL and the legacy `/macos/components/toolbars/` 404). macOS rows below are left open, not filled from memory. |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| bar container | Fluent: 48 px compact bar, commands 32 px tall | `Rectangle` `bar-height` default `toolbar-height` 48; `HorizontalLayout` with top/bottom padding `(bar-height − 32) / 2` so children are exactly `control-height` | ✅ | gallery "Bar (48 px)": all controls cut by the red centre line |
| dense variant | DESIGN §2: 24 px controls in dense areas | `bar-height: Theme.bar-height` (40 px) used for the search row and the transport bar; controls stay 32 px | ✅ | `library.slint` l. 223, `editor.slint` l. 493 |
| 40 px variant in gallery | DESIGN §4: every state in the gallery | only the 48 px bar is shown | ❌ | gallery |
| content area (title / text) | Fluent: content aligned left; Fluent example `TextBlock` in the bar | `BodyText` project title centred with `horizontal-stretch: 1` (editor toolbar); "Library" title + count left (library header) | ✅ | gallery `BodyText "Summer holiday •"` |
| primary commands | Fluent: right-aligned, added in order of importance | primary `ActionButton "Export…"` right of the title; library add-buttons right | ✅ | gallery |
| overflow / "see more" | Fluent: primary commands move to an overflow menu at breakpoints; Spectrum: overflow menu when space is constrained | none. Rows are planned to fit the minimum widths (window ≥ 900 px, library pane ≥ 280 px: two 96 px combos + 32 px button + paddings = 272 px) and long labels elide | ➖ fixed minimum sizes make every row fit; re-check when a row gains a control | manual: resize to 900 × 560 |
| separators between groups | Fluent `AppBarSeparator` | `VDivider` exists but no bar uses it; groups are separated by a stretch spacer | ➖ two groups per bar at most, spacer is enough | gallery |
| labels on commands | Fluent: icon + short label, compact mode hides labels; "Button labels should be short, preferably a single word" | icon-only `IconButton` with mandatory tooltip; text `ActionButton` for the primary action ("Export…", "Add to timeline") | ✅ | gallery |
| icon size in the bar | Fluent: 20 × 20 px in the primary area | `IconButton` glyph `Theme.icon-m` 20 px | ✅ | gallery |
| selected-item count (selection bar) | Spectrum ActionBar: count shown; DESIGN §3 bulk actions show the count in the action | "Add {} to timeline" carries the count; status bar shows "{} selected" | ✅ | `library.slint` l. 325–330, 400 |
| clear-selection button (selection bar) | Spectrum ActionBar: clear button + `onClearSelection` | none | ❌ | `library.slint` selection bar |
| selection bar visibility | Spectrum: hidden when `selectedItemCount` is 0 | always visible; actions disabled when nothing is selected, "Add all" stays usable | ➖ Fluent: keep a consistent command in a consistent location; "Add all" is valid without a selection | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Fluent: bar surface distinct from content | transparent by default; callers set `background: Theme.surface-pane` | ✅ | gallery (on `surface-pane`) |
| open / closed | Fluent: open state reveals labels and overflow | — no overflow, no open state | ➖ | |
| emphasised | Spectrum `isEmphasized` when floating over content | — bars are docked, not floating | ➖ | |
| empty content | Fluent: content area only when populated | children decide; no placeholder | ✅ | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | Fluent CommandBar 48 px compact | 48 default, 40 dense | ✅ |
| control height | 32 px (Spectrum M) | children padded to exactly 32 | ✅ |
| side padding | DESIGN §2: panel padding 16 | `side-padding` default `space-l` 16; editor toolbar uses 8 | ✅ (8 px chosen so the panel button sits under the divider) |
| spacing between controls | DESIGN §2: 8 between related controls | `spacing: Theme.space-s` | ✅ |
| icon size | 20 px | via `IconButton` | ✅ |
| separator | 1 px `Palette.border` | `VDivider`, unused in bars | ➖ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Tab reaches every command | Fluent: commands are focusable | every `IconButton` / `ActionButton` has a `FocusScope`; `ComboBox` and `SearchField` focusable | ✅ | manual |
| arrow keys move between commands inside the bar | Fluent CommandBar keyboard navigation | none; Tab only | ❌ | manual |
| Escape clears the selection | Spectrum ActionBar: Escape clears; DESIGN §3 selection model | no Escape handler in `library.slint`; editor `keys` scope has none either | ❌ | code |
| consistent command placement across pages | Fluent: "keep that command in a consistent location" | Export and Settings always top-right of the editor; Add files/folder always top-right of the library | ✅ | screenshots `empty`, `populated` |
| Accept left of Cancel | Fluent CommandBar recommendation | not applicable to bars; dialogs follow `DialogButtons` | ➖ | |
| labels collapse under pressure | Spectrum `buttonLabelBehavior: collapse` | text labels `overflow: elide`; no collapse to icon-only | ➖ min widths guarantee fit | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | toolbar / group | none set; Slint 1.18 exposes `groupbox` but no toolbar role | ❌ |
| label / tooltip | every icon-only command has a tooltip and label (DESIGN §2) | `IconButton` enforces both | ✅ |
| contrast (text 4.5:1, UI 3:1) | | children use `Palette`; not measured | ➖ unmeasured |
| focus visible | focus ring on each command | `IconButton`/`ActionButton` rings | ✅ |

## Wording

Bars carry no strings of their own. Strings of the controls inside are covered by their cards (`icon-button.md`, `action-button.md`, `search-field.md`) and by `std-widgets.md` for `ComboBox`.

## Gaps

1. **should** — No way to clear the selection from the selection bar or by keyboard (Spectrum ActionBar clear button, DESIGN §3 "Escape clears"). Fix: add Escape handling in `LibraryPage` (`LibraryState.clear-selection()`) and an `IconButton { icon: Icons.dismiss; tooltip: @tr("Clear selection") }` in the selection bar, shown when `selected-count > 0`.
2. **should** — The 40 px dense `Bar` is used twice but has no gallery row, so its centring has never been checked in isolation. Fix: add a `Row` with `Bar { bar-height: Theme.bar-height; … }` containing `IconButton`, `SearchField`, `Slider` and `Caption`.
3. **nice** — Arrow-key navigation between commands inside a bar (Fluent CommandBar). Fix: a `FocusScope` on `Bar` that moves focus with Left/Right when a child has focus; low priority while Tab order is short.
4. **nice** — No accessible group role/label on the bar. Fix: `accessible-role: groupbox` with an `in property <string> label` (e.g. "Library actions") once Slint exposes a fitting role; document as accepted otherwise.
5. **open** — Apple HIG "Toolbars" could not be fetched (JS-rendered). Re-run with a browser-backed fetch before the macOS pass; potential deviations (toolbar title position, item grouping) are unverified.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: Escape clears the selection in library and timeline (view-model callbacks); a quiet × button appears in the selection bar while items are selected; gallery row for the 40 px dense bar.
- Open: Apple HIG Toolbars page still unread (JS-rendered); arrow-key navigation between commands (nice).
