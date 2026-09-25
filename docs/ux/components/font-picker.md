# FontPicker (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/components.slint` → `FontPicker` (Text inspector)
Gallery rows: "FontPicker (value in its field; …)"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ComboBox.html | filter as you type ("contains"), arrows move, Enter selects, Escape closes; combo box when filtering a large list helps; always a label |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Field | text input with the value | 32 px field, `surface-card`, 1 px border (accent while focused) showing the family | ✅ | gallery, `text.png` |
| Trigger | chevron button | chevron icon 16 px, click opens the list | ✅ | gallery |
| List | filtered options in a popover | opens *inline* below the field (pushes the inspector down), up to 8 rows of 32 px, each family drawn in its own font. Not a `PopupWindow`: a popup takes the keyboard from the field, and closing it returned focus to the field, which reopened it — the app locked up (fixed 2026-09-25) | ✅ | `tests/ui_interaction.rs` |
| Empty result | message | "No fonts match" | ✅ | |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| filtering | "contains" | the app matches: prefix matches first, then containing, case-insensitive | ✅ | `font_search_puts_prefix_matches_first` |
| opening | on input (default) / manual | a click on the field or chevron opens it with the text selected, typing filters; it never reopens by itself; leaving the field closes it | ✅ | `the_font_picker_filters_picks_closes_and_keeps_the_app_usable` |
| ↑/↓, Enter, Escape | Spectrum | highlight moves (list scrolls with it), Enter picks, Escape closes and restores the value | ✅ | keyboard.md |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role, label, value | combobox with label | `combobox`, label "Font", value = family; list rows are list items with the highlight as selected | ✅ |

## Gaps

None open.
