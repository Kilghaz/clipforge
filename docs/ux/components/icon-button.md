# IconButton (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `IconButton`
Gallery rows: "IconButton: default, primary, checked, disabled, disabled-primary; guide shows the centre" and "Bar (48 px) with mixed controls" in `crates/app/ui/gallery.slint`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/buttons | button behaviour, ToggleButton, keyboard, text rules |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/command-bar | AppBarButton: 20 px icon, labels, bar anatomy |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/tooltips | when a tooltip is required, content, focus/hover trigger |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionButton.html | anatomy (icon, label), quiet variant, aria-label rule |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Tooltip.html | trigger (hover + focus), delay, placement, no disabled triggers |
| NN/G | https://www.nngroup.com/articles/icon-usability/ | icons need visible labels; only home/print/search are universal |
| NN/G | https://www.nngroup.com/articles/tooltip-guidelines/ | five tooltip rules (not vital info, brief, keyboard, arrows, consistency) |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| container | Fluent Button: filled control with 1 px border, radius 4; Spectrum ActionButton: bordered box | `Rectangle` 32 × 32, `Theme.radius-s`, 1 px `Theme.border`, `Theme.surface-card` | ✅ | gallery "IconButton" |
| icon | Fluent CommandBar: "size of the icons when shown in the primary command area is 20x20px"; Spectrum: icon child | `Icon` at `Theme.icon-m` (20 px), centred by x/y | ✅ | gallery "IconButton" guide row shows the centre |
| visible text label | NN/G: "a text label must be present alongside an icon to clarify its meaning"; Fluent CommandBar: "On larger windows, consider moving labels to the right of app bar button icons" | none; icon only with tooltip | ❌ | usages in `library.slint` (Add files…, Add folder…, Show details), `editor.slint` (Hide library) |
| tooltip | Fluent Tooltips: "Toolbar controls and command buttons showing only icons need tooltips"; Spectrum ActionGroup: icon-only "should usually include a tooltip" | `Tooltip { text: root.tooltip }`, property mandatory by convention | ✅ | all usages pass `tooltip:`; `DESIGN.md` §2 |
| accessible name | Spectrum: "an alternative text label must be provided … using the aria-label prop" | `accessible-label: root.tooltip` | ✅ | components.slint |
| focus ring | Fluent: FocusBorder outside the control | 2 px `Theme.focus-ring` rectangle, 2 px outside, radius + 2 | ✅ | components.slint (not in gallery, see States) |
| checked indicator | Fluent ToggleButton: accent background when `IsChecked`; `DESIGN.md` §2: "filled weight only for the on state of a toggle" | accent background, same regular-weight glyph | ❌ | gallery "IconButton" (checked mute) |
| quiet / borderless variant | Spectrum `isQuiet`: "displayed with a quiet style"; Fluent AppBarButton in a compact command bar is borderless until hover | no `quiet` prop; the gallery guide row overrides `background`/`border-color` inline | ❌ | gallery guide row |
| hold icon / chevron (menu) | Fluent DropDownButton, SplitButton; Spectrum ActionButton menu trigger | — no icon button opens a menu yet | ➖ | |
| repeat behaviour | Fluent RepeatButton (Delay, Interval) | — no press-and-hold command exists | ➖ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Fluent: control background, 1 px border | `Theme.surface-card`, `Theme.border` | ✅ | gallery "IconButton" |
| hover | Fluent Button: lighter control background; Spectrum: hover style | `Theme.control-hover` / `Theme.accent-hover` when `touch.has-hover`, 120 ms animation | ❌ | code only; no gallery row, no manual check (hover not simulable in the gallery) |
| pressed / active | Fluent Button pressed: tertiary background, text secondary | `Theme.control-pressed` / `Theme.accent-pressed`, glyph `Theme.text-secondary` | ❌ | code only; no gallery row, no manual check |
| focus (keyboard) | Fluent: visible focus border; Primer: never remove focus styles | ring drawn when `focus.has-focus` | ❌ | code only; no gallery row, no manual check |
| disabled | Fluent: disabled background, disabled text; not focusable | `Theme.control-disabled` / `Theme.accent-disabled`, glyph `Theme.text-disabled`; `FocusScope.enabled: false` | ✅ | gallery "IconButton" (Undo disabled, Play disabled-primary) |
| selected / checked | Fluent ToggleButton "can be on or off"; `IsChecked` toggles on click | `checkable` + `checked`, toggled in `touch.clicked`, `accessible-checked` | ✅ | gallery "IconButton" (Mute checked) |
| primary (accent) | Fluent: accent style Button | `primary` → `Theme.accent`, `Theme.on-accent` glyph | ✅ | gallery "IconButton" (Export) |
| checked + disabled | Fluent: accent-disabled | `accent` covers `checked`, so `Theme.accent-disabled` | ✅ | gallery "IconButton" disabled-primary uses the same path; no checked+disabled row |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | Fluent Button min 32 px; `DESIGN.md` size M 32 | `Theme.control-height` 32 px | ✅ |
| width / min click target | Fluent Button min 32 px; `DESIGN.md` min 24 × 24, touch 32 × 32 | 32 × 32 fixed | ✅ |
| radius | Fluent 4 px | `Theme.radius-s` 4 px | ✅ |
| border | Fluent 1 px control border | 1 px, transparent for accent | ✅ |
| icon size | Fluent CommandBar 20 × 20 (primary area) | `Theme.icon-m` 20 px | ✅ |
| type size / weight | — no text | — | ➖ |
| spacing to neighbours | not given on the fetched pages; `DESIGN.md` §2 "gap between related controls 8" | `Bar` spacing `Theme.space-s` 8 px | ✅ |
| bar height | Fluent CommandBar 48 px | `Theme.toolbar-height` 48 px | ✅ |
| motion | `DESIGN.md` 100–150 ms ease-out | `animate background 120 ms` | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Enter / Space activates when focused | Fluent: "pressing the Enter key or the Spacebar also raises the Click event" | `FocusScope.key-pressed` on `Key.Return` / `Key.Space` → `touch.clicked()` | ✅ | components.slint; no automated test |
| click on release | Fluent: default `ClickMode` is Release | `TouchArea.clicked` | ✅ | |
| toggle flips on click | Fluent ToggleButton | `if root.checkable { root.checked = !root.checked }` | ✅ | gallery "IconButton" |
| disabled is not focusable / clickable | Fluent disabled state | `FocusScope.enabled` and `TouchArea.enabled` follow `root.enabled` | ✅ | |
| tooltip appears on hover | Fluent: "display automatically when the user … hovers"; Spectrum default delay 1500 ms | Slint `Tooltip`; delay and placement owned by the Slint runtime | 🔒 | |
| tooltip appears on keyboard focus | Fluent: "when the user moves focus to"; Spectrum: "triggers via both focus and hover by default"; NN/G #3: "Tooltips that appear only on mouse hover are inaccessible for users that rely on keyboards" | Slint `Tooltip` is documented as hover-triggered only; nothing shows it on `focus.has-focus` | ❌ | not verifiable in the gallery |
| tooltip not shown on disabled trigger | Spectrum: "avoid disabled elements" as triggers | unverified: `Tooltip` sits in a `Rectangle` whose `TouchArea` is disabled | ❌ | needs a manual check |
| tooltip names the shortcut | Fluent: "make sure that it includes information about the keyboard accelerators" | "Undo (⌘Z)", "Play (Space)"; buttons without shortcuts omit it | ✅ | editor.slint |
| tooltip is supplemental, not vital | NN/G #1; Fluent: "text must be supplemental" | the tooltip is the only name of the action (see Anatomy: visible label) | ❌ | same gap as "visible text label" |
| tooltip does not repeat visible text | Fluent: "Don't use a tooltip to display text already visible" | icon only, so nothing is repeated | ✅ | |
| tooltip has no interactive content | Fluent, Spectrum | plain `@markdown` text | ✅ | |
| tooltip arrow | NN/G #4: "Arrows are beneficial … when multiple elements are nearby" | Slint `Tooltip` draws no arrow | 🔒 | Slint runtime owns the tooltip visuals |
| tooltips used consistently | NN/G #5 | every `IconButton` has one; `tooltip` is required for the accessible label | ✅ | components.slint |
| icon changes with state, tooltip follows | Fluent AppBarToggleButton; Spectrum icon per state | play/pause, sort direction, panel expand/contract, chevron all switch icon and tooltip together | ✅ | editor.slint, library.slint |
| no emoji as icons, Fluent icon set | `DESIGN.md` §2 | `Icons` global, Fluent UI System Icons regular | ✅ | `cargo xtask fluent-icons` |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | Fluent Button / ToggleButton; Spectrum `aria-pressed` for toggles | `accessible-role: button`, `accessible-checkable`, `accessible-checked` | ✅ |
| label / tooltip | Spectrum: aria-label required for icon-only | `accessible-label: root.tooltip`, always set | ✅ |
| enabled exposed | Spectrum `aria-disabled` | `accessible-enabled: root.enabled` | ✅ |
| default action | Fluent Click via automation | `accessible-action-default => touch.clicked()` | ✅ |
| contrast (icon 3:1 on control background, on-accent on accent) | `DESIGN.md` §5 | `Theme.text` on `Theme.surface-card` and `Theme.on-accent` on `Theme.accent` come from the style palette; not measured | ❌ |
| focus visible | Fluent FocusBorder; Primer "Show focus styles on keyboard :focus" | 2 px `Theme.focus-ring` (foreground colour) outside the box | ✅ |
| minimum target 24 × 24 | `DESIGN.md` §2 | 32 × 32 | ✅ |

## Wording

Tooltips are the only strings. All go through `@tr()` with German entries. Sentence case, verb first, ellipsis where a dialog opens:
"Settings", "Undo (⌘Z)" / "Undo (Ctrl+Z)", "Redo (⇧⌘Z)" / "Redo (Ctrl+Shift+Z)", "Hide library" / "Show library", "Play (Space)" / "Pause (Space)", "Add files…", "Add folder…", "Hide details" / "Show details", "Newest first. Click for oldest first" / "Oldest first. Click for newest first". Status: ✅ for all. The sort tooltip is two sentences; Fluent allows "short sentences and sentence fragments", so it passes, but it is the longest tooltip in the app.

## Gaps

1. **should** — No visible label; the tooltip is the only name (Anatomy "visible text label", Behaviour "tooltip is supplemental"). NN/G accepts only home, print and search as universally understood; undo/redo, play/pause and the gear are close to universal, but panel-left-contract, the sort arrows, document-add and folder-add are not. Fix: in the library toolbar use `ActionButton` with icon + text ("Add files…", "Add folder…") where the bar has room, or give `IconButton` an optional `label` shown to the right of the icon (Fluent `DefaultLabelPosition="Right"`); keep icon-only for undo/redo, play/pause, settings, close.
2. **should** — Tooltip does not appear on keyboard focus (Fluent, Spectrum and NN/G #3 all require it). Fix: show a small caption below the button while `focus.has-focus` (own `Rectangle` + `Text`, same string), or file the need upstream and record the limitation in `manual-checks.md`.
3. **nice** — Tooltip behaviour on a disabled button is unverified (Spectrum says do not attach tooltips to disabled triggers; Fluent shows them). Fix: check in the app, decide, and add the line to `manual-checks.md`.
4. **nice** — Checked state uses the regular-weight glyph; `DESIGN.md` §2 asks for the filled weight when a toggle is on. Fix: add `icon-checked` (filled SVG from `cargo xtask fluent-icons`) and use it when `checked`.
5. **nice** — No `quiet` variant although the gallery already needs one (guide row overrides background and border inline) and Fluent command bars are borderless. Fix: `in property <bool> quiet` → transparent background and border in the default state, `Theme.control-hover` on hover.
6. **nice** — Hover, pressed and focus states have no evidence: not in the gallery and not in `manual-checks.md`. Fix: add a `preview-state` debug property (`none | hover | pressed | focus`) that forces the state for the gallery, or add three lines to `manual-checks.md`.
7. **nice** — Contrast of the glyph on `surface-card` and of `on-accent` on `accent` has never been measured on the fluent and cupertino palettes. Fix: measure once from `target/screenshots/gallery.png` and record the ratios here.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: `quiet` variant added (used for the clear-selection button). Tooltips on the library filter/sort combos.
- Accepted: icon-only add-files / add-folder / sort buttons in the library header (the empty state offers the same actions as labelled buttons; header width is 280–500 px). Tooltip on keyboard focus is a Slint limitation → AUDIT-2026-09 #15.
- Open (nice): filled glyph for checked state; hover/pressed evidence needs a state-preview property.
