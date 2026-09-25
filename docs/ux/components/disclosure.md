# Disclosure (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/components.slint` → `Disclosure` (export dialog "Advanced")
Gallery rows: "Disclosure collapsed / expanded; ExportStatusButton …"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Primer | https://primer.style/product/ui-patterns/progressive-disclosure/ | use sparingly; chevron down when collapsed, up when expanded; pair the icon with text; keep context (content opens in place) |

## Anatomy / states

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| chevron | vertical chevron, down = collapsed, up = expanded | Fluent `chevron_down` / `chevron_up`, 16 px, secondary text colour | ✅ | gallery |
| label | describes the hidden content | "Advanced" / "Erweitert", body semibold | ✅ | gallery, export scenes |
| content | opens in place below | conditional `GridLayout` right under the row | ✅ | `export`, `narrow-export` |
| hover | — | `control-hover` tint, radius S | ✅ | |
| focus | visible ring | 2 px focus ring outside (`show-focus` for the gallery) | ✅ | gallery "… keyboard focus" |
| disabled | — | none needed (Advanced stays reachable while exporting, controls inside disable) | ➖ | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | DESIGN.md controls 32 | 32 | ✅ |
| padding | 4/8 scale | 4 left, 8 right, 8 between icon and text | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| click toggles | Primer | TouchArea | ✅ | `ui_interaction::the_export_dialog_discloses_advanced_and_gates_hdr` |
| Space / Enter toggles | button semantics | FocusScope | ✅ | keyboard.md |
| state remembered | — | bound to `EditorState.export-advanced-open` for the session | ✅ | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role and expanded state | aria-expanded | `accessible-role: button`, `accessible-expandable`, `accessible-expanded` | ✅ |
