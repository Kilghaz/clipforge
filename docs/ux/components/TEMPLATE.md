# <Component> (spec card)

Status: draft | audited YYYY-MM-DD
Implementation: `crates/app/ui/<file>.slint` → `<ComponentName>`
Gallery rows: "<row title>" in `crates/app/ui/gallery.slint`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | <url> | platform behaviour, metrics on Windows |
| Spectrum | <url> | anatomy, states, editor conventions |
| Primer | <url> | pattern / wording (if applicable) |
| NN/G | <url> | usability rule (if applicable) |
| Apple HIG | <url> | macOS deviation (if applicable) |

## Anatomy

Every element the references define. One row each; "—" if we
deliberately omit it, with the reason.

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| e.g. leading icon | Spectrum: magnifier, size S | `Icon` 16 px left | ✅ | gallery "SearchField" |
| e.g. help text | Spectrum: optional | — not needed, no validation | ➖ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | | | | |
| hover | | | | |
| pressed / active | | | | |
| focus (keyboard) | | | | |
| disabled | | | | |
| selected / checked | | | | |
| error / empty / loading (if any) | | | | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | | | |
| min width / padding | | | |
| radius | | | |
| icon size | | | |
| type size / weight | | | |
| spacing to neighbours | | | |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| e.g. Enter / Space activates | Fluent Button | FocusScope key handler | ✅ | manual-checks.md |
| e.g. Escape clears | Spectrum SearchField | | | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | | | |
| label / tooltip | | | |
| contrast (text 4.5:1, UI 3:1) | | | |
| focus visible | | | |

## Wording (if the component shows text)

Primer content rules that apply: sentence case, verb-first actions, ellipsis on
actions that open a dialog, no jargon. List the strings and their status.

## Gaps

Numbered list of every ❌ above, with severity (blocker / should / nice) and
the fix or the reason for accepting the deviation. Empty when audited clean.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.
