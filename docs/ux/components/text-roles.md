# Text roles, Section and dividers (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `SectionLabel`, `Caption`,
`BodyText`, `Section`, `Divider`, `VDivider`; type scale in
`crates/app/ui/theme.slint` (`Theme.font-*`, `Theme.weight-*`)
Gallery rows: "Text roles: title 16 / body 14 / secondary 12 / caption 11,
SectionLabel; Divider" and "Section + ValueSlider (inspector form), 280 px
wide; …" in `crates/app/ui/gallery.slint`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/style/typography (redirects to `signature-experiences/typography`) | type ramp, weights, minimum sizes, alignment, truncation, casing |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/style/spacing (redirects to `basics/content-basics`) | spacing between label/control/content, section headers in lists |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Form.html | label position and alignment, necessity indicator, form landmark |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Divider.html | divider sizes, orientation, accessibility |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Heading.html | heading levels, headings styled by their container |
| Primer | https://primer.style/product/ui-patterns/forms/ | label wording (≤ 3 words, sentence case), captions short, no placeholder-as-label |
| Primer | https://primer.style/product/getting-started/foundations/content/ | sentence case, no terminal punctuation on labels and headings |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Caption role | Fluent: Caption 12/16, weight Small; "use caption size for very confined spaces" | `Caption`: 11 px regular, `text-secondary`, elide | ❌ (size below Fluent minimum, see Metrics) | gallery "Text roles" |
| Body role | Fluent: Body 14/20 regular | `BodyText`: 14 px regular, `text`, elide | ✅ | gallery "Text roles" |
| Body strong (emphasis, list section header) | Fluent: Body Strong 14/20 semibold; "Use Body Strong for section headers" | `BodyText { font-weight: semibold }` ad hoc (library inspector title) | ✅ | `library.slint` inspector title |
| Secondary 12 role | Fluent: Caption 12 is the smallest ramp step | No component; raw `Text { font-size: Theme.font-secondary }` in `editor.slint` (time read-out, export status), `main.slint` (drag ghost) and the gallery | ❌ | gallery "Text roles" uses raw `Text` |
| Section / title 16 role | Fluent ramp has no 16; Spectrum Heading is styled by its container | No component; raw `Text { font-size: Theme.font-title }` in `EmptyState` and the gallery | ❌ | gallery "Text roles" uses raw `Text` |
| Dialog title 20 | Fluent: Subtitle 20/28 semibold | `DialogFrame` title 20 semibold | ✅ | gallery "DialogFrame" |
| Field / section label | Spectrum Form: label above field (`labelPosition="top"`, `labelAlign="start"`); Primer: descriptive, ≤ 3 words, sentence case | `SectionLabel`: 12 px semibold, `text-secondary`, left-aligned above children | ❌ (12 px semibold is below Fluent's 14 px semibold minimum) | gallery "Section + ValueSlider" |
| Section container | Spectrum Form: fields stacked; Fluent: 8 epx between control and header | `Section`: `SectionLabel` + children, spacing 8 | ✅ | gallery "Section + ValueSlider" |
| Necessity indicator | Spectrum Form: asterisk or "(required)" | — no required fields in inspectors or dialogs | ➖ | |
| Help text / caption under a field | Primer: as short as possible, not duplicating the label | `Caption { wrap }` under the trim section ("Drag the edges of a video clip to trim it.") | ✅ | `editor.slint` line 316 |
| Placeholder as label | Primer: never | `SearchField` placeholder is the only placeholder; it has an accessible placeholder, no visible label (search fields are the accepted exception) | ✅ | search-field card |
| Horizontal divider | Spectrum: sizes S/M/L (L default), horizontal default | `Divider`: 1 px `Theme.border` | ✅ | gallery "Text roles" |
| Vertical divider | Spectrum: `orientation="vertical"` | `VDivider`: 1 px `Theme.border` | ✅ | gallery "Text roles" |
| Divider thickness variants | Spectrum: S/M/L | — one thickness; a 1 px hairline is all a dense dark editor needs | ➖ | |
| Heading levels | Spectrum Heading: levels 1–6, default 3 | — Slint has no heading role or level | ➖ | |

## States

Text roles and dividers are static; only colour states apply.

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | Fluent: text colour from theme | `Theme.text` (foreground) / `Theme.text-secondary` (62 % foreground) | ✅ | gallery "Text roles" |
| disabled | Fluent: disabled text brush | `Theme.text-disabled` (36 % foreground) exists; none of the text roles takes an `enabled` prop, callers switch colour by hand (`SearchField` does) | ❌ | none in gallery |
| selected (text in selection) | Fluent: selection foreground | only `TextInput` in `SearchField`; not applicable to labels | ➖ | |
| hover / pressed / focus | not applicable to static text | — | ➖ | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| Caption size | Fluent: 12 epx; "Minimum values 14px Semibold, 12px Regular … smaller … illegible in some languages" | 11 px regular | ❌ |
| Secondary size | Fluent: 12 epx (Caption) | 12 px regular | ✅ |
| Body size | Fluent: 14/20 | 14 px | ✅ |
| Section title size | Fluent: no 16 step (Body Strong 14 or Body Large 18) | 16 px semibold (DESIGN.md §2 scale) | ➖ deliberate ClipForge scale step between body and dialog title; Spectrum owns editor typography |
| Dialog title | Fluent: Subtitle 20/28 semibold | 20 px semibold | ✅ |
| Weights | Fluent: Regular for most text, Semibold for titles; no Bold, no Italic | 400 / 600 only | ✅ |
| Line height | Fluent: 16 / 20 / 28 | — Slint `Text` has no line-height property; font default | ➖ |
| Section label weight/size | Fluent minimum for semibold is 14 px | 12 px semibold | ❌ |
| Alignment | Fluent: left by default, centre only under icons | roles default left; library cell captions centred under thumbnails, `EmptyState` centred | ✅ |
| Line length | Fluent: 50–60 characters per line | not constrained; see empty-state card | ➖ per screen |
| Truncation | Fluent: ellipsis, wrap when multi-line | `Caption`, `BodyText`: `overflow: elide`; `SectionLabel`: default (clip) | ❌ (SectionLabel) |
| Label → control gap | Fluent: 8 epx between control and header | `Section.spacing` 8 | ✅ |
| Side label → control gap | Fluent: 12 epx between control and label | `ExportWindow` Advanced rows: spacing 12 | ✅ |
| Section → section gap | Fluent: 12 epx between content areas; DESIGN.md (Spectrum): 24 | 24 in inspector and gallery | ✅ (Spectrum owns the spacing scale per DESIGN.md §1) |
| Divider thickness | Spectrum: S/M/L | 1 px | ✅ |
| Divider colour | Fluent/Spectrum: border colour | `Theme.border` (= `Palette.border`) | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| Static text is not focusable | Fluent | `Text` is not focusable | ✅ | — |
| Form is a landmark with a label | Spectrum Form: `aria-label` / `aria-labelledby` required | `Section` sets no accessible role or label; the `SectionLabel` is a plain text | ❌ | — |
| Divider is a separator | Spectrum: accepts `aria-label`; functions as separator | — Slint has no separator role; decorative `Rectangle` | ➖ | — |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | Slint `Text` reports `text`; heading and group roles do not exist in Slint | ➖ |
| label / tooltip | n/a for static text | ➖ |
| contrast text ≥ 4.5:1 | `text` white on fluent dark surfaces ≫ 4.5:1; `text-secondary` ≈ 6.9:1 on `#202020` (estimate from 62 % alpha, not measured); `text-disabled` ≈ 3.2:1 (disabled text is exempt) | ✅ (estimated, no automated check) |
| minimum legible size | Fluent 12 regular / 14 semibold | Caption 11 regular and SectionLabel 12 semibold below | ❌ (Metrics) |
| focus visible | n/a | ➖ |

## Wording

Primer: labels ≤ 3 words, sentence case, no terminal punctuation on labels
and headings; captions short and not repeating the label.

| String (source) | Rule | Status |
|---|---|---|
| "Photo duration", "Framing", "Transition", … (`Section.title`, editor) | ≤ 3 words, sentence case, no punctuation | ✅ |
| "Drag the edges of a video clip to trim it." (Caption help text) | full sentence → terminal punctuation allowed | ✅ |
| "Changes apply immediately." (settings caption) | full sentence | ✅ |
| "Type", "Dimensions", "Duration", "Captured", "File size", "Status" (inspector `InfoRow` labels) | ≤ 3 words, sentence case | ✅ |
| Gallery texts ("Title 16 semibold", …) | not translated by design (gallery is internal) | ➖ |

## Gaps

1. **should** — `Caption` is 11 px regular, below Fluent's stated legibility
   minimum of 12 px regular (Metrics, Anatomy). Fix: either raise
   `Theme.font-caption` to 12 px and merge it with `font-secondary`
   (DESIGN.md §2 then lists four sizes), or record the 11 px step as an
   accepted Spectrum-scale deviation in DESIGN.md with the German-fit check
   as the guard. Decide once; today the card and the guide disagree with the
   platform source.
2. **should** — `SectionLabel` is 12 px semibold; Fluent's minimum for
   semibold text is 14 px, and Fluent uses Body Strong (14 semibold) for
   section headers. Fix: `SectionLabel { font-size: Theme.font-body }`
   (keep `text-secondary` and semibold), re-render the inspector at 280 px
   and the German strings.
3. **should** — The "secondary 12" and "title 16" roles have no component;
   screens use raw `Text { font-size: Theme.font-secondary / font-title }`
   (`editor.slint` transport read-out and export status, `main.slint` drag
   ghost, `EmptyState` title). DESIGN.md says "the three text roles; no other
   sizes in screens", so either add `SecondaryText` and `Title` components
   and use them everywhere, or drop the two sizes from the scale. Add both
   to the gallery "Text roles" row as components, not raw `Text`.
4. **nice** — `SectionLabel` has no `overflow: elide`, so a long German label
   in a 280 px inspector clips without an ellipsis (Fluent: ellipsis). Fix:
   add `overflow: elide` like `Caption`/`BodyText`.
5. **nice** — No disabled variant for the text roles; callers that dim text
   (e.g. `SearchField` placeholder) branch on colour by hand. Fix: add
   `in property <bool> enabled: true` to `Caption`/`BodyText` mapping to
   `Theme.text-disabled`, and show a disabled row in the gallery.
6. **nice** — `Section` exposes no accessible grouping (Spectrum Form
   requires a labelled landmark). Fix: set `accessible-role: groupbox` (if
   the pinned Slint version offers it) and `accessible-label: root.title` on
   `Section`; otherwise record the Slint limitation here.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: `Title` and `SecondaryText` components added and used (library header, inspector heading, transport time, export status); `SectionLabel` elides.
- Decided: 11 px caption and 12 px semibold section labels stay (Spectrum scale; recorded in DESIGN.md §2 as an accepted Fluent deviation).
- Open (nice): disabled variant, Section accessible grouping.
