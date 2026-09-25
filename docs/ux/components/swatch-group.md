# SwatchGroup (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/components.slint` → `SwatchGroup` (title card background)
Gallery rows: "SongRow: …; SwatchGroup default / keyboard focus / disabled; …"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ColorSwatchPicker.html | predefined palette, single selection, unique colours, localised names, sizes, rounding |
| Fluent 2 | radio buttons (M4 decision) | one tab stop, arrow keys for a single choice |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| swatch | colour tile, sizes XS–L | 24 px tile inside a 32 px cell, `radius-s`, 1 px `text-disabled` edge (brightens on hover) so dark swatches (Black, Charcoal) stand out from the dark pane | ✅ | gallery |
| selection indicator | selected swatch marked | 2 px accent ring with a 4 px gap (reads on black and on white) | ✅ | gallery row 1–2 |
| group label | aria-label "Color swatches", overridable | `label` → accessible group name ("Title background") | ✅ | `editor.slint` |
| colour names | localised names | `names` → tooltip and accessible label per swatch (Black, Charcoal, Blue, Red, White) | ✅ | gallery |
| unique colours | required | five distinct tokens `title-bg-*` | ✅ | `theme.slint` |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | | | ✅ | gallery |
| hover | | border brightens | ✅ | |
| selected | | accent ring | ✅ | gallery |
| focus (keyboard) | focus visible | ring switches to `focus-ring` while the group has focus | ✅ | gallery row 2 |
| disabled | | 40 % opacity, no input | ✅ | gallery row 3 |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| cell | M size ≈ 32 px | 32 px (`control-height`) | ✅ |
| spacing | density regular | 8 px | ✅ |
| rounding | "none" default, "full" only on one row | `radius-s` (DESIGN radius scale) | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| single selection | Spectrum | `select(i)` | ✅ | |
| one tab stop, arrows / Home / End move | Fluent radio group | FocusScope key handler | ✅ | keyboard.md |
| change applies at once, one undo step | DESIGN §3 | `SetTitleBackground` | ✅ | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| group role and name | | groupbox + label | ✅ |
| swatch name, checked state | | button, checkable, checked | ✅ |
| colour not the only cue | | names in tooltips and for screen readers | ✅ |

## Gaps

None open.
