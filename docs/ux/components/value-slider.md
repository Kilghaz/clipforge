# ValueSlider (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/editor.slint` → `ValueSlider` (wraps `std:Slider` from `std-widgets.slint`; the timeline zoom slider in the transport bar uses a bare `std:Slider` and is covered in the behaviour rows below)
Gallery rows: "Section + ValueSlider (inspector form), 280 px wide; EmptyState; DialogButtons Windows / macOS" and the bare `Slider` in "SearchField empty / with text / disabled; std ComboBox, CheckBox, Slider, ProgressIndicator at control-height" in `crates/app/ui/gallery.slint`
Used in: inspector "Photo duration" (0.5–20 s, step 0.5), "Volume" (0–200 %, step 5, disabled while muted), "Transition duration" (0.2–3 s, step 0.1, disabled for "Cut")

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/slider | when a slider is right (relative quantity, ≥ 4 values, instant feedback) vs numeric text box (exact value, tight space, keyboard users); labels without ending punctuation; range labels; value label with units; tick marks when snap points are not obvious; disable associated labels with the slider; do not resize the thumb |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Slider.html | anatomy (label, value label, track, handle, fill), `labelPosition` top/side, `showValueLabel`, `getValueLabel` formatting, keyboard (arrows, Home/End, Page Up/Down), a label or aria-label is required |
| NN/G | https://www.nngroup.com/articles/gui-slider-controls/ | sliders only where an approximate value is good enough; labels beside or above so they stay visible while dragging; offer tap/type alternatives where the exact value matters |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| label | Fluent: label with no ending punctuation, above the slider in forms with labels above; Spectrum `label` top (default) | `Section.title` above ("Photo duration", "Audio", "Transition into clip"); `ValueSlider.label` feeds only the accessible label | ✅ | gallery "Section + ValueSlider" |
| value label | Fluent: "display it below the slider … include the units"; Spectrum: value label on the label row (top) or side; NN/G: beside or above, never under the finger | `Text` right of the track, 48 px, right-aligned, units included ("4 s", "50 %") | ✅ (Spectrum/NN/G side placement wins over Fluent "below") | gallery |
| value formatting | Spectrum `getValueLabel` | caller supplies `value-text` (`round(x*10)/10 + " s"`) | ✅ | code |
| track | Fluent/Spectrum | std `Slider` track | 🔒 | |
| handle / thumb | Fluent: "Don't change the size of the slider thumb from the default size" | std default | 🔒 | |
| fill (min → value) | Spectrum `isFilled` optional | std fluent slider fills from the left | 🔒 | gallery |
| range labels (min / max) | Fluent: "Label the two ends of the slider range" (one word each, parallel) | none on `ValueSlider`; the zoom slider uses `zoom-out` / `zoom-in` icons at both ends (Fluent's "speaker graphic" pattern) | ❌ (ValueSlider) / ✅ (zoom) | `editor.slint` l. 505–517 |
| tick marks | Fluent: show ticks when step points are not obvious (e.g. 10 snap points on 200 px); hide when they are dense | none; std `Slider` has no tick API. Duration: 39 steps over ~170 px, Volume: 40, Transition: 28 → snapping is barely visible | ➖ steps are dense enough per Fluent's own example; significant values are offered as `ChoiceButton` presets | gallery |
| presets / typed input alternative | NN/G: offer tap or type where exact values matter; Fluent: numeric text box if the user prefers the keyboard | `ChoiceButton` row 2/3/4/5/8 s under the duration slider; no typed input on any slider | ✅ (duration) / ❌ (volume, transition) | gallery |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | | std | 🔒 | gallery |
| hover | Fluent: thumb inner grows | std fluent `hover` state | 🔒 | |
| pressed / dragging | std | 🔒 | |
| focus (keyboard) | focus visible | std fluent: thumb inner shrinks to 10 px on focus (same as pressed), no ring | 🔒 | |
| disabled | Fluent: "Disable all associated labels or feedback visuals when you disable the slider" | `enabled` forwarded to std `Slider`; value text → `Theme.text-disabled`. The `Section` title above stays in normal colour | ✅ (value label) | code |
| disabled in gallery | DESIGN §4: all states in the gallery | only the enabled slider is shown | ❌ | gallery |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | control 32 px | `HorizontalLayout` takes the std Slider min-height 20 px; sits in a `Section` with 8 px spacing | 🔒 |
| min width | Fluent: "Make sure the endpoints … fit within the bounds of a view" | stretches to the inspector width (296 − 32 − 48 − 8 ≈ 208 px track) | ✅ |
| radius / thumb / track thickness | style | std | 🔒 |
| type size / weight | value label same scale as body/secondary | `Theme.font-secondary` 12 px, regular | ✅ |
| value label width | fits the longest value in DE and EN | 48 px fits "20 s", "200 %", "3.0 s" | ✅ |
| spacing to neighbours | 8 between related controls | `spacing: Theme.space-s` between track and value | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| right control choice | Fluent: relative quantity, ≥ 4 values, instant feedback; NN/G: approximate is fine | Duration (0.5–20 s): the user thinks "shorter/longer" and presets cover exact values ✅. Volume 0–200 %: relative ✅. Transition 0.2–3 s: relative ✅ | ✅ | |
| live value read-out while dragging | Fluent: immediate feedback; NN/G: value visible while dragging | `value <=> state`, `value-text` recomputed on every change | ✅ | manual |
| commit on release, one undo step | DESIGN §3: single undo step per gesture | `released(v)` calls `duration-changed` etc., which applies one `Command` | ✅ | manual-checks.md M2 "duration slider changes every clip in one undo step" |
| keyboard commit | same rule for keyboard edits | std `slider-base` fires `released` on `key-released` for arrows/Home/End, so keyboard edits also commit | ✅ | `i-slint-compiler` `widgets/common/slider-base.slint` |
| arrow keys step by `step` | Spectrum / Fluent | std: Left/Right ± step | ✅ | std |
| Home / End | Spectrum | std | ✅ | std |
| Page Up / Page Down (larger step) | Spectrum | std has none | ❌ | std |
| step aligns with max | Fluent: final step aligns to max | 0.5→20 (step 0.5) ✅, 0→200 (5) ✅, 0.2→3 (0.1) ✅ | ✅ | |
| direction | Fluent: low on the left for LTR | std | 🔒 | |
| not used as a progress indicator | Fluent | progress uses `ProgressIndicator` | ✅ | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | slider | std `accessible-role: slider`, value/min/max/step, increment/decrement actions | ✅ (std) |
| label | Spectrum: label or aria-label required | `accessible-label: root.label` set by every caller; zoom slider has `accessible-label` + `Tooltip` | ✅ |
| value announced with units | | `accessible-value` is the raw float; the unit lives only in the visual text | ➖ std limitation; role carries min/max |
| contrast | | std palette | 🔒 |
| focus visible | | std thumb change only, no ring | 🔒 |

## Wording

| String | Rule | Status |
|---|---|---|
| "Photo duration", "Volume", "Transition duration", "Timeline zoom" | sentence case, no punctuation; DE present | ✅ |
| value texts "4 s", "50 %", "1.2 s" | unit with a space; decimal point not localised (DE would use comma) | ❌ |

## Gaps

1. **should** — No disabled `ValueSlider` in the gallery although two of three usages have a disabled state. Fix: add `ValueSlider { enabled: false; … value-text: "100 %"; label: "Volume"; }` to the Section row.
2. **should** — Decimal separator is hard-coded (`round(x*10)/10 + " s"` renders "1.2 s" in German). Fix: format the value in Rust (`format.rs`) per language and expose `duration-text` / `transition-text` properties instead of computing in Slint.
3. **nice** — No range labels on `ValueSlider` (Fluent: label both ends). Fix: optional `min-text` / `max-text` props rendered as `Caption` under the track ends, or icon ends like the zoom slider.
4. **nice** — No Page Up / Page Down (Spectrum). Fix: a `FocusScope` in `ValueSlider` that forwards PageUp/PageDown as ±10 × step via the std slider's `value`; or upstream to Slint.
5. **nice** — Volume and transition have no exact-value alternative (NN/G). Fix: presets (`ChoiceButton` 0.5 / 1 / 2 s; 50 / 100 / 150 %) or a `SpinBox` next to the value text.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: disabled ValueSlider in the gallery.
- Deferred: decimal separator per language → AUDIT-2026-09 #11; exact-value alternative → #17.
