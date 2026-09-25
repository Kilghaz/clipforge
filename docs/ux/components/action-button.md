# ActionButton (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `ActionButton` (inherits the std `Button` from `std-widgets.slint`; pins `height`, `min-width`, `colorize-icon`)
Gallery rows: "ActionButton: text, icon+text, primary, primary icon+text, disabled; …" , "Bar (48 px) with mixed controls", "DialogFrame … with DialogButtons", "Section + ValueSlider … EmptyState; DialogButtons Windows / macOS" in `crates/app/ui/gallery.slint`

Std source read for the 🔒 rows: `i-slint-compiler-1.18.1/widgets/fluent/button.slint` (min 32 × 32, radius 4, 1 px border, padding 12 / 5, icon 20 px, spacing 4, hover / pressed / checked / disabled states, `FocusBorder`, Enter / Space). The cupertino variant was not read; `DESIGN.md` §4 records that it rounds more.

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/buttons | behaviour, text rules (120 px min width, 26 chars), order, keyboard |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Button.html | variants (accent / primary / secondary / negative), fill / outline, pending state, icon + label |
| Primer | https://primer.style/components/button/guidelines | one primary per page, label rules, focus styles, inactive vs disabled. (`https://primer.style/product/ui-patterns/button-usage/` was fetched three times and returned only the site navigation; it is JS-rendered.) |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| container | Fluent: filled control, 1 px border, radius 4 | std `Button` background + border | 🔒 | gallery "ActionButton" |
| text label | Fluent: "content is usually text … a single word that is a verb"; Spectrum: string or `<Text>` child | `text` property, centred `Text` | ✅ | gallery "ActionButton" |
| leading icon | Spectrum: icon + `<Text>`; Fluent: "use an icon in addition to text" | `icon` property, 20 px, `colorize-icon: true` so it takes the text colour | ✅ | gallery "Show in folder", "Export…" |
| icon-only form | Spectrum: icon-only "must include aria-label"; std `Button` cannot centre a lone icon | forbidden; `IconButton` is used instead | ✅ | `crates/app/tests/ui_tokens.rs` "std Button with icon; use IconButton or ActionButton" |
| focus border | Fluent FocusBorder; Primer: "Don't remove default Button :focus styles" | std `FocusBorder` when `has-focus && enabled` | 🔒 | |
| accent / primary variant | Fluent accent style; Spectrum `variant="accent"`; Primer: "Only use one primary Button on a page, whenever possible" | `primary: true` | ✅ | gallery "Add to timeline", "Export…" |
| secondary / default variant | Spectrum `secondary` "most commonly used"; Fluent standard | default | ✅ | gallery "Add all" |
| negative / danger variant | Spectrum `variant="negative"`; Primer `danger` | — none. "Remove from timeline" and "Remove from library" are one undo step (`DESIGN.md` §3: undo instead of warning) | ➖ | |
| outline / quiet style | Spectrum `style="outline"`, Primer `invisible` | — not needed; every text button sits on a pane or dialog with its own surface | ➖ | |
| pending indicator | Spectrum `isPending`: "After a 1 second delay, an indeterminate spinner will be displayed in place of the button label" | — no button runs long work; "Export…" opens a dialog and the export job reports progress in the dialog (`DESIGN.md` §3) | ➖ | |
| counter / selection count in label | Primer: "Use the '2 selected' format" | "Add {} to timeline", "Remove {} from timeline" show the count | ✅ | library.slint, editor.slint |
| checkable form | Fluent ToggleButton; std `Button.checkable` exists | — not used; `ChoiceButton` covers selectable options with platform-independent visuals | ➖ | |
| chevron / split | Fluent DropDownButton, SplitButton | — no button opens a menu | ➖ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Fluent control background | std | 🔒 | gallery "ActionButton" |
| hover | Fluent: "secondary" control background | std `hover when i-touch-area.has-hover` (read in the fluent source) | 🔒 | none: not simulable in the gallery, not in `manual-checks.md` |
| pressed / active | Fluent: tertiary background, secondary text | std `pressed when root.pressed` | 🔒 | none, as above |
| focus (keyboard) | Fluent FocusBorder | std `FocusBorder` | 🔒 | none, as above |
| disabled | Fluent: disabled fill and text; not focusable | std `disabled when !root.enabled`; `FocusScope.enabled <=> root.enabled` | 🔒 | gallery "Remove" (disabled) |
| primary + disabled | Fluent accent-disabled | std `accent-disabled` path | 🔒 | no gallery row ("Add to timeline" is disabled with nothing selected in the app, so the state is common) ❌ |
| checked | Fluent ToggleButton accent | std `checked` state | ➖ | not used, see Anatomy |
| pending / loading | Spectrum `isPending` | — | ➖ | see Anatomy |
| inactive (Primer) | Primer: "an accessible alternative to a disabled Button … can respond to user input", e.g. "Show a tooltip on hover or focus" | disabled buttons give no reason ("Add to timeline" with nothing selected, "Export…" with an empty timeline) | ❌ | library.slint, editor.slint |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | Fluent min 32 px; `DESIGN.md` size M 32 | `height: Theme.control-height` (fixed, so the button cannot stretch to its layout cell) | ✅ |
| min width, short text | Fluent: "For shorter text, avoid narrow command buttons by using a minimum button width of 120px" | `min-width: 80px`; "Add all" renders at exactly 80 px | ❌ |
| max text length | Fluent: "limiting text to a maximum length of 26 characters" | English max 23 ("Remove from timeline"); German "{} aus der Zeitleiste entfernen" = 31, "{} zur Timeline hinzufügen" = 27 | ❌ |
| text wrap on overflow | Fluent option 1 / 2: widen or wrap | std `Text` has no wrap or elide; fixed height prevents wrapping | ❌ |
| padding | Fluent std 12 px horizontal, 5 px vertical | std | 🔒 |
| radius | Fluent 4 px (= `Theme.radius-s`); cupertino rounds more | std | 🔒 |
| icon size | Fluent CommandBar 20 × 20 | std `icon-size` default 20 px | 🔒 |
| icon-to-text spacing | not on the fetched pages | std 4 px | 🔒 |
| type size / weight | Fluent: "Use the default font" | std default font | 🔒 |
| spacing to neighbours | `DESIGN.md` §2: 8 px related, 16 px groups | `Bar` and `DialogButtons` spacing `Theme.space-s` 8 px | ✅ |
| motion | `DESIGN.md` 100–150 ms | std 150 ms | 🔒 |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Enter / Space activates | Fluent: "pressing the Enter key or the Spacebar also raises the Click event" | std `FocusScope.key-pressed` (read in the fluent source) | ✅ | no automated test |
| click on release | Fluent `ClickMode=Release` | std `TouchArea.clicked` | ✅ | |
| disabled is not focusable | Fluent | std `FocusScope.enabled <=> root.enabled` | ✅ | |
| one primary per view | Primer: "Only use one primary Button on a page, whenever possible" | library bar "Add to timeline" (primary) and editor toolbar "Export…" (primary) are both visible in the main window | ❌ | populated scene |
| primary at the end of a group | Primer: "place primary buttons at the end of a ButtonGroup"; Fluent: "OK/[Do it]/Yes" first | `DialogButtons` follows the platform: Windows primary first, macOS primary last (`DESIGN.md` §1: Fluent owns platform behaviour) | ➖ | gallery "DialogButtons Windows / macOS" |
| one or two buttons per decision | Fluent: "Expose only one or two buttons to the user at a time" | dialogs: primary + Cancel; inspector sections: one or two | ✅ | export window |
| single button alignment | Fluent: right-align in dialogs and containers, left-align on pages | `DialogButtons` `alignment: end`; inspector buttons left in `Section` | ✅ | gallery "Section + ValueSlider" |
| verb describes the action, not "OK" | Fluent; `DESIGN.md` §3 | "Export…", "Add to timeline", "Remove from library" | ✅ | |
| button used for navigation | Fluent: use HyperlinkButton for navigation | no `ActionButton` navigates | ✅ | |
| button stays aligned in a bar | `DESIGN.md` §4 `ActionButton` note | fixed `height` | ✅ | gallery "Bar (48 px)" |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | Fluent Button; Spectrum | std `accessible-role: button` | ✅ |
| label / tooltip | Spectrum: visible label is the name | std `accessible-label: root.text`; icon has `accessible-role: none` | ✅ |
| enabled exposed | Spectrum `aria-disabled` | std `accessible-enabled` | ✅ |
| default action | | std `accessible-action-default` | ✅ |
| contrast (text 4.5:1, UI 3:1) | `DESIGN.md` §5 | style palette (`control-foreground` on `control-background`, `accent-foreground` on `accent-background`); not measured | 🔒 |
| focus visible | Fluent FocusBorder; Primer | std `FocusBorder` | 🔒 |

## Wording

Primer: sentence case, succinct, no line breaks, verb first; Fluent: usually one verb, ≤ 26 characters; `DESIGN.md` §3: ellipsis on actions that open a dialog.

| String (EN) | German | Status |
|---|---|---|
| "Add all" | — (not measured) | ✅ |
| "Add to timeline" / "Add {} to timeline" | "Zur Timeline hinzufügen" (24) / "{} zur Timeline hinzufügen" (27) | ❌ German exceeds 26 with the count |
| "Show in folder" | "Im Ordner anzeigen" (18) | ✅ |
| "Remove from library" | "Aus Mediathek entfernen" (23) | ✅ |
| "Select all" | "Alle auswählen" (15) | ✅ |
| "Remove from timeline" / "Remove {} from timeline" | "Aus Timeline entfernen" (22) / "{} aus der Zeitleiste entfernen" (31) | ❌ length and inconsistent term (Timeline vs Zeitleiste) |
| "Rotate 90°" | "Um 90° drehen" (14) | ✅ |
| "Export…" | ellipsis, opens dialog | ✅ |
| "Cancel" | | ✅ |
| "Add folder…", "Add files…" (EmptyState) | ellipsis, open pickers | ✅ |

## Gaps

1. **should** — `min-width` is 80 px; Fluent asks for 120 px for short-text command buttons ("Add all", "Cancel", "Remove"). Fix: set `min-width: 120px` in `ActionButton` and check the library bar and `DialogButtons` still fit at the narrow scene (900 px), or record 80 px as an accepted deviation in `DESIGN.md` §2 with the reason (dense bars).
2. **should** — German labels break the 26-character rule and the std `Text` neither wraps nor elides, so the label overflows the button at the fixed 32 px height: "{} aus der Zeitleiste entfernen" (31), "{} zur Timeline hinzufügen" (27). Fix: shorten to "{} entfernen" / "{} hinzufügen" (the section label already says Timeline) and make "Zeitleiste" consistent with "Timeline" in the `.po`; add a gallery row with the longest German label.
3. **should** — Two primary buttons in one window ("Add to timeline" in the library bar, "Export…" in the toolbar). Fix: make "Add to timeline" a default button (the count in the label already carries the emphasis) or keep it primary only while the library panel has focus; keep "Export…" as the single accent action.
4. **nice** — Disabled buttons give no reason (Primer "inactive" pattern). Fix: tooltip on the disabled "Add to timeline" ("Select photos or videos first") and "Export…" ("Add clips to the timeline first"), via a `disabled-hint` property that renders a `Tooltip`.
5. **nice** — No gallery row for primary + disabled, although it is the most common state of "Add to timeline" and "Export…". Fix: add `ActionButton { text: "Export…"; primary: true; enabled: false; }` to the row.
6. **nice** — Hover, pressed and focus are style-owned but have no evidence anywhere. Fix: three lines in `manual-checks.md` (fluent and cupertino), or a `preview-state` debug property as for `IconButton`.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: "Add to timeline" demoted to a default button (one primary per window). German labels shortened to "{} hinzufügen" / "{} entfernen"; "Timeline" unified to "Zeitleiste" across the .po. Gallery rows for primary+disabled and the longest German label.
- Decided: `min-width` 96 px in panes, 120 px in dialogs (recorded in DESIGN.md §2).
- Open (nice): disabled-reason tooltip.
