# ChoiceButton (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `ChoiceButton`
Gallery rows: "ActionButton: … ChoiceButton selected / default / disabled; …" and "Section + ValueSlider (inspector form), 280 px wide" (five justified presets) in `crates/app/ui/gallery.slint`
Used in: `crates/app/ui/editor.slint` "Photo duration" presets 2 / 3 / 4 / 5 / 8 s, `horizontal-stretch: 1`, `selected: duration-seconds == s`.

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionGroup.html | anatomy of a button group, single selection, `disallowEmptySelection`, density, `isJustified`, `isEmphasized`, roles (`toolbar` / `radiogroup`), arrow-key navigation |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/radio-button | when a small set of exclusive options is right (2–8), default selection, label rules, keyboard: arrows move, selection follows focus, Tab lands on the selected item |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/buttons (ToggleButton section) | "immediately switch between two mutually exclusive states"; consider RadioButton when the UI does not benefit from a button |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| group container | Spectrum ActionGroup: "a grouping of ActionButtons that are related to one another", one `aria-label`, `role="radiogroup"` for single selection; Fluent `RadioButtons` with `Header` | plain `HorizontalLayout` in `editor.slint`; the `Section` title "Photo duration" is visual only, no accessible group or label | ❌ | editor.slint |
| item container | Spectrum ActionButton box; Fluent: button visuals for ToggleButton | `Rectangle` 32 px high, `Theme.radius-s`, 1 px `Theme.border`, `Theme.surface-card` | ✅ | gallery "ChoiceButton" |
| text label | Fluent: "Limit the radio button's text label to a single line"; parallel wording | `Text`, `Theme.font-body`, centred, single line | ✅ | gallery "2 s … 8 s" |
| icon option | Fluent `RadioButtons` with `SymbolIcon`; Spectrum `Item` with icon | — presets are numbers; no icon needed | ➖ | |
| selected indicator | Spectrum `isEmphasized` accent fill "only applied when items are selected"; Fluent ToggleButton accent | `Theme.accent` background, `Theme.on-accent` text, border transparent | ✅ | gallery "4 s" selected |
| focus ring | Fluent FocusBorder | 2 px `Theme.focus-ring` outside the box | ✅ | code only (see States) |
| horizontal padding | Spectrum ActionButton has inner padding; Fluent text buttons 12 px | none: `min-width: 40px`, label `width: parent.width`; longer text than ~30 px would touch or overflow the border | ❌ | "8 s" fits by luck of the short labels |
| merged borders (compact density) | Spectrum `density="compact"`: "reduces the margin size between the buttons" and merges borders | separate boxes, 4 px gap (`Theme.space-xs`) = regular density | ✅ | gallery "Section + ValueSlider" |
| overflow / collapse to menu | Spectrum `overflowMode="collapse"`, `summaryIcon` | — fixed five short items at 280 px; never overflows | ➖ | |
| tooltip | Spectrum: icon-only groups need tooltips | — text labels are visible | ➖ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Spectrum ActionButton default | `Theme.surface-card`, `Theme.border` | ✅ | gallery "5 s" |
| hover | Spectrum hover; Fluent hover | `Theme.control-hover` / `Theme.accent-hover` on `touch.has-hover` | ❌ | code only; no gallery row, no manual check |
| pressed / active | Spectrum down; Fluent pressed | `Theme.control-pressed` / `Theme.accent-pressed` | ❌ | code only; no gallery row, no manual check |
| focus (keyboard) | Fluent: focus visible; Tab lands on the selected item | ring when `focus.has-focus`; each button is its own Tab stop | ❌ | code only; see Behaviour |
| disabled | Spectrum `isDisabled` / `disabledKeys`; Fluent disabled | `Theme.control-disabled`, `Theme.text-disabled`; not focusable | ✅ | gallery "8 s" disabled |
| selected | Spectrum `selectionMode="single"`; Fluent `IsChecked` | `selected` in, `accessible-checked` | ✅ | gallery "4 s" |
| selected + hover / pressed | Spectrum emphasized hover | `Theme.accent-hover` / `Theme.accent-pressed` | ❌ | code only |
| selected + disabled | Spectrum disabled selected | `Theme.accent-disabled`, `Theme.on-accent-disabled` | ❌ | code only; no gallery row |
| none selected | Fluent: "In the default state, no radio button … is selected"; once selected cannot be cleared by the user | when the slider sets a value off the presets (e.g. 4.5 s) no preset is selected | ✅ | editor.slint |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | `DESIGN.md` size M 32 | `Theme.control-height` 32 px | ✅ |
| min width | not on the fetched pages (Fluent 120 px applies to command buttons, not toggles) | `min-width: 40px`; justified in the inspector | ✅ |
| padding | Spectrum ActionButton inner padding; `DESIGN.md` §2 "component internal padding 8 or 12" | none (see Anatomy) | ❌ |
| radius | Fluent 4 px | `Theme.radius-s` | ✅ |
| gap between items | Spectrum regular density gap; `DESIGN.md` §2 "gap between related controls 8" | `Theme.space-xs` 4 px | ❌ |
| type size / weight | `DESIGN.md` body 14 | `Theme.font-body` 14, regular | ✅ |
| motion | `DESIGN.md` 100–150 ms | `animate background 120 ms` | ✅ |
| compact padding (M5) | `DESIGN.md` §2 "component internal padding 8 or 12" | `compact` on `ChoiceGroup`/`ChoiceButton`: 8 px sides instead of 12, so four caption styles fit 264 px in English and German | ✅ | gallery "ChoiceGroup compact" |
| justified width | Spectrum `isJustified`: "divide all available horizontal space evenly among the buttons" | `horizontal-stretch: 1` per item in `editor.slint` | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| exactly one selected | Spectrum `selectionMode="single"`; Fluent radio semantics | selection is derived from the model (`duration-seconds == s`), so never two | ✅ | editor.slint; `EditorState` view-model tests |
| cannot deselect by clicking the selected item | Spectrum `disallowEmptySelection`; Fluent: "can't be cleared if the user selects it again" | clicking the selected preset sets the same value again | ✅ | editor.slint |
| Enter / Space activates focused item | Fluent: Spacebar selects the focused item | `FocusScope.key-pressed` Return / Space → `touch.clicked()` | ✅ | components.slint |
| arrow keys move within the group | Fluent: "The Left and Up arrow keys move to the previous item, and the Right and Down arrow keys move to the next item"; Spectrum ActionGroup: arrow-key navigation within the group | none; each item is an independent Tab stop | ❌ | |
| selection follows focus (arrows select) | Fluent: "as focus moves from one item to the next, the newly focused item gets selected"; Ctrl + arrows move without selecting | none | ❌ | |
| Tab enters the group on the selected item | Fluent: "When a user tabs into the list where a radio button is already selected, the selected radio button gets focus" | Tab visits every item in order | ❌ | |
| no focus wrap at the ends | Fluent: "doesn't wrap focus from the first row or column to the last" | n/a until arrows exist | ➖ | |
| 2–8 options, else combo box | Fluent: "If there are more than eight options, use a combo box"; not for a binary choice | five presets | ✅ | |
| default selection present | Fluent: `SelectedIndex` to provide a default; Spectrum `defaultSelectedKeys` | project default 4 s is a preset | ✅ | core `Project` default |
| all options deserve equal attention | Fluent: "Radio buttons emphasize all options equally" | yes; presets are shortcuts for the slider above | ✅ | |
| only one group in a row | Fluent: "Don't put two RadioButtons groups side by side" | one group per section | ✅ | |
| one command, one undo step | `DESIGN.md` §3 | `EditorState.duration-changed(s)` | ✅ | view-model tests |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| group role and label | Spectrum: `role="radiogroup"` for single selection, `aria-label` on the group; Fluent Narrator: "name, RadioButton, selected, x of N" | none; items are unrelated `button`s to assistive tech | ❌ |
| item role | Fluent RadioButton; Spectrum radio within radiogroup | `accessible-role: button` + `accessible-checkable: true` + `accessible-checked` (a toggle button, not a radio) | ❌ |
| item label | Fluent: `AutomationProperties.Name` or ToString | `accessible-label: root.text` = "4 s" (no unit word) | ❌ |
| enabled exposed | Spectrum `disabledKeys` | `accessible-enabled` | ✅ |
| default action | | `accessible-action-default => touch.clicked()` | ✅ |
| contrast (text 4.5:1) | `DESIGN.md` §5 | `Theme.text` on `surface-card`, `on-accent` on `accent`, from the style palette; not measured | ❌ |
| focus visible | Fluent | 2 px ring, foreground colour | ✅ |

## Wording

Labels are `s + " s"` built in `editor.slint` without `@tr()`. The unit symbol "s" is the same in German, so nothing is untranslated, but the string does not follow CLAUDE.md rule 7 literally and the accessible name reads "4 s". Status: ➖ for the visible label (SI unit, identical in both languages), ❌ for the accessible name (see Accessibility).

## Gaps

1. **should** — No accessible group: the five presets are unrelated toggle buttons to a screen reader, with no "Photo duration" name and no "x of N". Fix: wrap the `HorizontalLayout` in `editor.slint` in a `Rectangle { accessible-role: groupbox; accessible-label: @tr("Photo duration"); }` or add a `ChoiceGroup` component in `components.slint` that owns the layout, the label and (gap 2) the keyboard.
2. **should** — No arrow-key navigation and no selection-follows-focus; Tab stops on every preset. Fix: in the `ChoiceGroup` from gap 1, one `FocusScope` for the group: Left/Up and Right/Down move the selection and emit `clicked` for the new item, Tab leaves the group, focus enters on the selected item.
3. **should** — No horizontal padding: the label spans `parent.width`, so any text longer than about 30 px collides with the border; only the two-character presets hide it. Fix: compute `min-width: max(40px, label.preferred-width + 2 * Theme.space-m)` and give the `Text` `x: Theme.space-m; width: parent.width - 2 * Theme.space-m`.
4. **nice** — Gap between items is 4 px (`Theme.space-xs`) while `DESIGN.md` §2 says 8 px between related controls; Spectrum regular density also separates items visibly. Fix: `spacing: Theme.space-s` in `editor.slint` (and the gallery row), or document 4 px as the compact-density exception for justified groups.
5. **nice** — Accessible name is "4 s". Fix: `accessible-label: @tr("{} seconds", s)` passed from the caller via an `accessible-text` property, keeping the short visible label.
6. **nice** — Hover, pressed, selected + hover and selected + disabled have no evidence in the gallery or `manual-checks.md`. Fix: add a `ChoiceButton { text: "3 s"; selected: true; enabled: false; }` row now; hover / pressed via a `preview-state` debug property shared with `IconButton`.
7. **nice** — Text contrast on `surface-card` and `on-accent` on `accent` not measured on either palette. Fix: measure once from `target/screenshots/gallery.png` and record here.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: `ChoiceGroup` component: accessible group with label, one tab stop, Left/Right/Up/Down/Home/End move the selection, focus ring on the selected option; per-option `accessible-text` ("4 seconds"). Horizontal padding from label width. Gallery rows for selected+disabled and a long label. Used for duration presets, framing, orientation, export resolution and quality.
- Decided: 4 px gap inside a group is the compact-density exception for justified option sets.
