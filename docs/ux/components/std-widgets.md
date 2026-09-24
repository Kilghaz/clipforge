# Std widgets used as-is (spec card)

Status: audited 2026-09-24
Implementation: `std-widgets.slint` → `std:ComboBox`, `std:CheckBox`, `std:ProgressIndicator`, `std:MenuBar` (Slint 1.18, fluent on Windows / cupertino on macOS). Visuals are the style's (🔒); this card covers what is ours: **which control** for each usage, **labels**, and the **behaviour we drive**.
Gallery rows: "SearchField empty / with text / disabled; std ComboBox, CheckBox, Slider, ProgressIndicator at control-height" (ComboBox, CheckBox checked, ProgressIndicator determinate + indeterminate); ComboBox also in "Bar (48 px)…" and "Section + ValueSlider…". MenuBar: window-level, seen in the `empty` / `populated` scenes of `cargo run -p clipforge-app --bin screenshot`.

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/combo-box | fewer than five items → consider radio buttons; two options where one implies "not the other" → check box or toggle; combo box for secondary choices; single-line items; most common options first; header; keyboard text search |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/checkbox | check box = status, may be committed later; toggle switch = action, committed immediately; label as a statement the check mark makes true; ≤ 2 lines; indeterminate only for partial groups; not for commands or on/off controls |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/progress-controls | determinate when the end is predictable; indeterminate bar = non-blocking unknown duration; ring = blocking; a line of text may still be needed; no progress control when the user does not need to know |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/menus | menu bar for multiple top-level menus (File, Edit, View, Help); item kinds: action, sub-menu, toggle, radio, separator; icons only for common / well-known commands at 16 px; light dismiss |

## Usages and the right-control question

| Where | Control | Options | Reference rule | Status | Evidence |
|---|---|---|---|---|---|
| Library filter (`library.slint` l. 236) | ComboBox | All / Photos / Videos / Audio (4) | Fluent: < 5 items → consider radio; combo box when the choice is secondary and space is tight | ✅ pane is 280 px wide; filter is secondary to the grid; four `ChoiceButton`s would not fit next to sort + direction | gallery "Bar" ComboBox |
| Library sort (`library.slint` l. 245) | ComboBox + `IconButton` direction | Date / Name / Type / Added (4) | same; most common option first (Date) | ✅ | |
| Inspector "Framing" (`editor.slint` l. 320) | ComboBox | Fit inside (bars) / Fill frame (crop) (2) | Fluent: a list with the same 2 options → check box / toggle, or radio for 2–4 static items; a drop-down hides the alternative | ❌ | gallery "Section": ComboBox "Fit inside (bars)" |
| Inspector "Transition into clip" (`editor.slint` l. 331) | ComboBox | Cut / Cross dissolve / Fade through black (3) | Fluent: 3–4 static items → consider radio | ❌ | |
| Inspector "Project" (`editor.slint` l. 352) | ComboBox | Landscape 16:9 / Portrait 9:16 (2) | 2 static options → radio / toggle group | ❌ | |
| Export "Resolution" (`editor.slint` l. 382) | ComboBox | Full HD (1080p) / 4K (2160p) (2) | 2 static options → radio group | ❌ | `export` scene |
| Export "Quality" (`editor.slint` l. 387) | ComboBox | Good / Better / Best (3) | 3 static items → consider radio; secondary choice with a recommended default may stay a combo | ➖ default "Better" is recommended for most users; Fluent allows the combo to "minimize distraction" | |
| Settings "Interface language" (`settings.slint` l. 22) | ComboBox | dynamic list from Rust | combo box for single-line dynamic lists | ✅ | `settings` scene |
| Inspector "Mute" (`editor.slint` l. 301) | CheckBox | on/off, commits immediately via `toggled` → command | Fluent: "Don't use a check box as an on/off control"; immediate commit → toggle switch (std `Switch` exists) or a checkable `IconButton` (speaker-mute) | ❌ | gallery CheckBox "Mute" |
| Export "Optimize for YouTube" (`editor.slint` l. 389) | CheckBox | option applied when "Export" is pressed | Fluent: check box = status, committed later with the form | ✅ | `export` scene |
| Export "HDR (available once HDR sources are supported)" (`editor.slint` l. 390) | CheckBox, always disabled | never checkable | Fluent: label is a statement the check mark makes true; a permanently disabled option with an explanation in its label is neither status nor action | ❌ | `export` scene |
| Library status bar (`library.slint` l. 407) | ProgressIndicator 80 px | indeterminate while scanning, determinate `done / total` while adding | Fluent: indeterminate bar for unknown non-blocking work, determinate once the total is known, with a text line ("Scanning…", "Adding {} of {}…") | ✅ | gallery both variants |
| Export dialog (`editor.slint` l. 392) | ProgressIndicator | determinate `export-progress`, text "Exporting… {}%", Cancel button | Fluent determinate bar; DESIGN §3: > 10 s cancellable | ✅ | `export` scene |
| Window menu (`main.slint` l. 45) | MenuBar | File / Edit / View | Fluent: menu bar for a horizontal row of top-level menus; example set File, Edit, View, Help | ✅ (structure) | `empty` scene |

## Labels and wording

| Control | Reference | Ours | Status |
|---|---|---|---|
| ComboBox header / visible label | Fluent: a combo box "can show a header"; the closed state shows only the current selection | Inspector combos sit under a `Section` title ✅; export combos have a side `BodyText` label ✅; library filter and sort have **no visible label**: closed they read "All" and "Date", which do not say filter/sort | ❌ (library) |
| ComboBox accessible label | | `accessible-label` set on every ComboBox ("Filter by type", "Sort by", "Framing", …) | ✅ |
| ComboBox items single line, sorted logically | Fluent | all items single-line; sort: Date first (most common), sizes ascending, quality ascending | ✅ |
| ComboBox default selection | Fluent: default recommended for most users | every combo has a bound `current-index` with a default | ✅ |
| CheckBox label as a true statement | Fluent | "Mute", "Optimize for YouTube" ✅; "HDR (available once …)" mixes state and an explanation | ❌ (HDR) |
| CheckBox label ≤ 2 lines | Fluent | DE "HDR (verfügbar, sobald HDR-Quellen unterstützt werden)" fits one line at 480 px | ✅ |
| Progress text alongside the bar | Fluent: "a line of text is still necessary" | "Scanning…", "Adding {} of {}…", "Exporting… {}%" | ✅ |
| Menu item capitalisation | DESIGN §3: sentence case for everything | File menu mixes "New Project", "Save Project As…" (title case) with "Add files to library…", "Add folder to library…" (sentence case); Edit has "Select All Clips", "Remove Selected Clips"; View "Hide Library" | ❌ |
| Ellipsis on items that open a dialog | Primer / DESIGN §3 | "Open Project…", "Save Project As…", "Export Video…", "Add files to library…", "Add folder to library…", "Settings…" ✅; "New Project", "Save Project", "Undo", "Quit" act directly ✅ | ✅ |
| Platform wording for exit | Fluent example "Exit"; macOS "Quit" | "Quit" on both platforms | ❌ |
| German entries | CLAUDE.md rule 7 | all strings above have `msgstr` entries | ✅ |

## Behaviour we drive

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| ComboBox selection commits a command | DESIGN §3 undo everything | `selected => …-changed(index)` applies a `Command` for framing, transition, aspect; filter/sort/language are view state (not undoable, correctly) | ✅ | proptests in `core` |
| ComboBox keyboard: Up/Down, Return, Escape | Fluent | std `combobox-base` | ✅ (std) | |
| ComboBox type-ahead text search | Fluent: type a letter to jump | std has none | ➖ lists have ≤ 4 items | |
| ComboBox disabled during export | Fluent: disable what cannot change | `enabled: EditorState.export-status != 1` on both export combos and the YouTube box | ✅ | |
| CheckBox Space / Return toggles | Fluent | std | ✅ (std) | |
| CheckBox indeterminate | Fluent: only for partial groups | not used; no grouped checkboxes | ➖ | |
| Progress shown after > 1 s, cancel after > 10 s | DESIGN §3 | export: cancellable ✅; library scan / import: **no cancel** anywhere in `library.slint` although folder scans can run minutes | ❌ | code |
| Indeterminate → determinate transition | Fluent: smooth transition when the total becomes known | `indeterminate: status-kind == 1` flips to determinate at import start | ✅ | |
| Progress control not interactive | Fluent: read-only | std | ✅ | |
| Menu items disabled when not applicable | Fluent | Undo/Redo bound to `can-undo`/`can-redo`; Export and Select All to `clip-count > 0`; Remove to `selected-count > 0` | ✅ | |
| Toggle item for a binary view option | Fluent `ToggleMenuFlyoutItem` | View → "Hide Library" / "Show Library" swaps the title; Slint 1.18 `MenuItem` has no checkable property (checked `i-slint-compiler/widgets/fluent/menu.slint`) | ➖ text swap until Slint supports checkable items | |
| Shortcuts shown in menus | DESIGN §3: "shortcuts are listed in `docs/ux/keyboard.md` and in the menus" | none shown; Slint 1.18 `MenuItem` has no shortcut/accelerator property. Shortcuts are handled in `EditorPage.keys` (Ctrl/Cmd+Z, Y, A, Delete, Space, J/K/L, arrows, Home) and only while the editor has focus | ❌ | `main.slint`, `editor.slint` l. 423–440 |
| Separators group related items | Fluent | File: project / export / library / settings / quit; Edit: history / selection | ✅ | |
| Icons in menus | Fluent: only for common, well-known commands | none | ➖ | |
| Help menu | Fluent example includes Help → About | none; "About" lives in Settings | ❌ | |

## Visuals

All rows about radius, fill colours, check glyph, popup shadow, focus ring, thumb and bar thickness: 🔒 owned by the fluent / cupertino style. Radius 4 px matches `Theme.radius-s` (DESIGN §4). The gallery shows the std widgets only for alignment against `control-height`; unchecked and disabled `CheckBox`, disabled `ComboBox` are not shown (❌, nice: for comparison only).

## Gaps

1. **should** — Two-option choices use a ComboBox: Framing (Fit/Fill), Project orientation (Landscape/Portrait), Export resolution (1080p/4K). Fluent: 2–4 static options → radio group; the drop-down hides the alternative. Fix: a `ChoiceButton` pair (`Spectrum Action group`, already used for duration presets) in the inspector; in the Export dialog the same pair right of the side label.
2. **should** — "Mute" is an immediate on/off control as a CheckBox (Fluent: use a toggle). Fix: std `Switch { text: @tr("Mute") }` or a checkable `IconButton { icon: Icons.speaker-mute }` next to the Volume slider; keep the `toggled → muted-changed` command path.
3. **should** — Library filter and sort ComboBoxes have no visible label; closed they show "All" / "Date". Fix: a `Caption` prefix in the row ("Show", "Sort by") or a `Tooltip` on each ComboBox using the existing accessible label strings; check the 280 px pane still fits (elide item text).
4. **should** — Menu items mix title case and sentence case. Fix: sentence case throughout per DESIGN §3 ("New project", "Open project…", "Save project", "Save project as…", "Export video…", "Select all clips", "Remove selected clips", "Hide library" / "Show library"); update the `.po` entries.
5. **should** — Shortcuts are not visible in the menus and only work while the editor has focus. Fix (interim): append the shortcut to the title via `Shell.macos` (`@tr("Undo") + "\t" + (Shell.macos ? "⌘Z" : "Ctrl+Z")`) as the toolbar tooltips already do; move the Ctrl/Cmd handlers to `MainWindow` so they work with library focus too; revisit when Slint adds accelerators.
6. **should** — Library scan / import shows progress but cannot be cancelled (DESIGN §3: > 10 s cancellable; CLAUDE.md rule 4). Fix: an `IconButton { icon: Icons.dismiss; tooltip: @tr("Cancel import") }` in the status bar while `status-kind == 1 || 2`, wired to a `LibraryState.cancel-import()` that triggers the job's `CancellationToken`.
7. **nice** — Transition kind (3 options) as a ComboBox. Fix: `ChoiceButton` row once the transition palette (M4, key `T`) lands; DE "Schwarzblende" must fit 296 px.
8. **nice** — Permanently disabled "HDR (available once HDR sources are supported)" CheckBox. Fix: remove it, or show a `Caption` "HDR export follows once HDR sources are supported." (Primer: explain degraded features in prose, not in a dead control).
9. **nice** — "Quit" on Windows should read "Exit" (Fluent menu example). Fix: `Shell.macos ? @tr("Quit") : @tr("Exit")`.
10. **nice** — No Help menu (Fluent example: Help → About). Fix: add Help → "About ClipForge" once there is more than the version string; accept until then.
11. **nice** — Gallery lacks unchecked / disabled `CheckBox` and disabled `ComboBox` for alignment comparison. Fix: extend the std row.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: two- and three-option ComboBoxes replaced by `ChoiceGroup` (framing, orientation, export resolution, quality); Mute and Optimize-for-YouTube are `Switch`es; permanently disabled HDR checkbox replaced by a caption; menu items sentence-cased; "Exit" on Windows, "Quit" on macOS; filter/sort combos carry tooltips.
- Deferred: import cancel → AUDIT-2026-09 #10; shortcuts in menus → #12; Help menu → #16.
