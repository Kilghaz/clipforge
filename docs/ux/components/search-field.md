# SearchField (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `SearchField`
Gallery rows: "SearchField empty / with text / disabled; std ComboBox, CheckBox, Slider, ProgressIndicator at control-height" in `crates/app/ui/gallery.slint`
Used in: `crates/app/ui/library.slint` (library search row, stretches to the pane width)

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/text-box | platform behaviour: clear-all button, placeholder vs header, focus/edit behaviour, context menu |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/auto-suggest-box | search box anatomy (query icon, clear all, "No results" line) |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/SearchField.html | anatomy (search icon, clear button), Escape clears, Enter submits, aria-label required without a visible label |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/TextField.html | states (disabled, read-only, invalid, quiet, focus), label position |
| NN/G | https://www.nngroup.com/articles/search-visible-and-simple/ | search is a type-in field not a link; wide enough for the typical query; default scope "all", state a narrowed scope |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| search icon (leading) | Spectrum: search icon at the start; Fluent AutoSuggestBox: optional `QueryIcon="Find"` on the right | `Icon` `Icons.search` 16 px, `text-secondary`, left | ✅ | gallery "SearchField empty" |
| input | Fluent TextBox single line; Spectrum input | `TextInput` `single-line: true`, 14 px | ✅ | gallery |
| placeholder | Fluent: "Use a label or placeholder text if the purpose of the text box isn't clear"; placeholder disappears once a value is entered | `placeholder` prop, hidden when `text != ""`, `text-secondary` | ✅ | gallery "empty" vs "with text" |
| clear button | Fluent: "clear all" X appears when text is entered (TextBox: only while focused); Spectrum: clear button when field has content | 24 × 24 button with `Icons.dismiss`, shown whenever `text != ""` (Spectrum rule, not tied to focus) | ✅ | gallery "with text" |
| clear button while disabled | Fluent: clear-all not shown for read-only; disabled control must not act | `if root.text != ""` is not gated on `enabled`; `clear-touch` has no `enabled` binding, so a disabled field with text still clears on click | ❌ | code: `components.slint` SearchField |
| visible label / header | Fluent: header optional; Spectrum: label or `aria-label` mandatory | none visible; placeholder acts as label | ➖ single search in a pane titled "Library"; see accessibility row | |
| help text / description / error | Spectrum TextField: optional | — no validation on a filter | ➖ | |
| suggestions list | Fluent AutoSuggestBox | — filtering happens live in the grid, no suggestions needed | ➖ | |
| "No results" line | Fluent AutoSuggestBox: show a single-line "No results" so users know the search ran | `EmptyState` "Nothing matches your search" + "Try another word or set the type filter to \"All\"." in `library.slint` | ✅ | `library.slint` l. 318–320 |
| focus line | Fluent TextBox: accent underline when focused | 2 px `Theme.accent` bottom line + accent border on `input.has-focus` | ✅ | code (not simulable in gallery) |
| context menu (cut/copy/paste/select all) | Fluent TextBox: "built-in context menu with support for copying and pasting text" | none; Slint `TextInput` has shortcuts only | ❌ | manual |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Fluent: 1 px border, control background | `surface-card`, 1 px `Theme.border`, radius 4 | ✅ | gallery "empty" |
| hover | Fluent TextBox: background lightens on hover | no hover state (no `TouchArea` on the frame) | ❌ | gallery cannot show; code |
| focus (keyboard) | Fluent: accent underline; Spectrum: focus ring | accent border + 2 px accent bottom line, background becomes `surface-window`; motion 120 ms | ✅ | code |
| with text | Spectrum/Fluent: clear button visible | clear button visible, placeholder hidden | ✅ | gallery "with text" |
| disabled | Spectrum `isDisabled`; Fluent disabled TextBox dims text and fill | `input.enabled: false`, placeholder/text use `text-disabled`; frame background and border unchanged, so the field reads as enabled in the render | ❌ | gallery "disabled" (looks like "empty") |
| read-only | Spectrum `isReadOnly`, Fluent `IsReadOnly` | — not needed for a filter | ➖ | |
| invalid / error | Spectrum `validationState` | — no validation | ➖ | |
| quiet | Spectrum `isQuiet` | — one variant only | ➖ | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | DESIGN §2: 32 px (Spectrum size M) | `Theme.control-height` 32 px | ✅ |
| min width / padding | Fluent: "appropriate width for the range of values"; NN/G: wide enough for the typical query (mean 2 words) | `min-width: 120px`; in the library it stretches to the pane width (≥ 280 − 32 px). Padding 8 left / 4 right, spacing 8 | ✅ |
| radius | DESIGN §2: 4 px controls | `Theme.radius-s` | ✅ |
| icon size | DESIGN §2: 16 px in dense controls | `Theme.icon-s` for magnifier and dismiss | ✅ |
| type size / weight | body 14 | `Theme.font-body`, regular | ✅ |
| clear button target | DESIGN §2: min 24 × 24 | `control-height-s` 24 × 24 | ✅ |
| spacing to neighbours | gap 8 between related controls | inherited from `Bar` spacing 8 | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| typing filters live | Fluent AutoSuggestBox `TextChanged`; Spectrum `onChange` | `edited` → `LibraryState.search-changed` on every edit | ✅ | manual-checks.md M1 "Search, type filter and sort react instantly" |
| Enter submits | Spectrum `onSubmit`; Fluent `QuerySubmitted` | — not needed, results are already live; Enter is not swallowed | ➖ | |
| Escape clears | Spectrum: Escape clears and fires `onClear` | Escape clears and emits `edited("")` only when text is non-empty; otherwise rejected so the dialog/pane can handle it | ✅ | code |
| clear button clears and refocuses | Spectrum `onClear` on clear button | sets `text = ""`, emits `edited("")`, calls `input.focus()` | ✅ | code |
| clear button keyboard reachable | Spectrum: clear button is not in the tab order; Escape is the keyboard path | not focusable; Escape covers it | ➖ Escape is the keyboard path | |
| shortcut to focus search | Apple HIG / Fluent convention Cmd/Ctrl+F (DESIGN §3: every mouse action reachable by keyboard) | none; not in `docs/ux/keyboard.md` | ❌ | keyboard.md |
| default scope "all" | NN/G: default search scope "all"; state a narrowed scope | type filter defaults to "All"; the empty state names the filter as a cause | ✅ | `library.slint` |
| edit rather than replace on focus | Fluent: default places the caret, selects nothing | Slint default | ✅ | |
| copy / paste shortcuts | Fluent TextBox: supported by default | Slint `TextInput` supports Ctrl/Cmd+C/V/X/A | ✅ | manual |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | text input | `TextInput` default role `text-input` | ✅ |
| label | Spectrum: "an aria-label must be provided to the SearchField" when there is no visible label | `accessible-placeholder-text` only; no `accessible-label` on the input or the root | ❌ |
| clear button label | icon-only buttons need a label and tooltip (DESIGN §2) | `accessible-role: button`, `accessible-label: @tr("Clear search")`, `Tooltip` | ✅ |
| contrast (text 4.5:1, UI 3:1) | | text `Palette.foreground`; placeholder `foreground` at 62 % alpha on `control-background` — not measured | ➖ unmeasured; DESIGN checklist item | |
| focus visible | Fluent: accent underline | accent border + underline | ✅ |

## Wording

| String | Rule | Status |
|---|---|---|
| "Search the library" (placeholder) | sentence case, no punctuation; DE "Mediathek durchsuchen" | ✅ |
| "Clear search" (tooltip/label) | verb-first; DE "Suche löschen" | ✅ |
| "Nothing matches your search" / "Try another word or set the type filter to \"All\"." | Primer empty state: what + next step; DE present | ✅ |

## Gaps

1. **should** — Clear button stays active when `enabled: false`. Fix: `if root.text != "" && root.enabled : Rectangle { … }` and `clear-touch.enabled: root.enabled`.
2. **should** — No accessible label on the input (Spectrum requires `aria-label` without a visible label). Fix: add `in property <string> label: root.placeholder;` and set `input.accessible-label: root.label`.
3. **should** — Disabled state is visually indistinguishable from default (only the placeholder dims). Fix: add a `disabled when !root.enabled` state with `background: Theme.control-disabled`, `border-color: Theme.border`, and hide the magnifier tint to `text-disabled`.
4. **nice** — No hover state on the frame (Fluent TextBox lightens on hover). Fix: add a `TouchArea` behind the layout and `hover` state with `background: Theme.control-hover`.
5. **nice** — No shortcut to focus the search field. Fix: handle Cmd/Ctrl+F in `LibraryPage` (or `MainWindow`) → `search.focus()`, add the row to `docs/ux/keyboard.md`.
6. **nice** — No context menu (cut/copy/paste/select all) although Fluent lists it as built-in. Fix: add a right-click `ContextMenuArea` with the four items once the app has a context-menu pattern (see menus card); accept until then.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.
