# IconChoiceGroup (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/components.slint` → `IconChoiceGroup` (text alignment)
Gallery rows: "FontPicker …; IconChoiceGroup alignment: left / centre (selected) / right, disabled"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionGroup.html | text alignment as a single-selection group; icon-only buttons need tooltips / labels; one tab stop, arrow keys |

## Anatomy / states

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| items | icon buttons | `IconButton` 32 px, Fluent `text_align_*` icons 20 px, 4 px apart | ✅ | gallery |
| selected | emphasised | checked (accent) style | ✅ | gallery |
| labels | tooltip + accessible name per item | "Align left", "Centre", "Align right" | ✅ | |
| disabled | | all items disabled | ✅ | gallery |
| focus | ring on the selected item while the group has focus | `show-focus` on the selected `IconButton` | ✅ | |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| one tab stop, ←/→/Home/End move the selection | Spectrum ActionGroup | group `FocusScope`; items not focusable on their own | ✅ | keyboard.md |

## Gaps

None open.
