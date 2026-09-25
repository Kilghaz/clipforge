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
| List | filtered options in a popover | `PopupWindow`, up to 8 rows of 32 px, each family drawn in its own font (a preview) | ✅ | manual-checks M5 |
| Empty result | message | "No fonts match" | ✅ | |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| filtering | "contains" | the app matches: prefix matches first, then containing, case-insensitive | ✅ | `font_search_puts_prefix_matches_first` |
| opening | on input (default) / focus | opens on focus with all families, then filters while typing | ✅ | |
| ↑/↓, Enter, Escape | Spectrum | highlight moves (list scrolls with it), Enter picks, Escape closes and restores the value | ✅ | keyboard.md |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role, label, value | combobox with label | `combobox`, label "Font", value = family; list rows are list items with the highlight as selected | ✅ |

## Gaps

None open.
