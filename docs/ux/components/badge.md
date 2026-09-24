# Badge, KindBadge and HDR tag (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `Badge`;
`crates/app/ui/library.slint` → `KindBadge` (inherits `Badge`) and the
inline HDR tag (`Rectangle` + `Text "HDR"`, inspector); muted badge use in
`crates/app/ui/editor.slint` (`ClipView`)
Gallery rows: "Badges: video, audio, cloud, failed, muted; HDR tag";
badges in context in "Library cells: …" and "Timeline clips (100 px): …"
in `crates/app/ui/gallery.slint`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/info-badge | when a badge is the wrong control, icon badge metrics (16 px), placement inside the parent's bounds, accessibility via the parent |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Badge.html | anatomy (icon, label, both), semantic vs category colours, `aria-label` when icon-only |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/StatusLight.html | semantic colour vocabulary (positive / negative / notice / info / neutral), `role="status"` for runtime changes |
| NN/G | https://www.nngroup.com/articles/icon-usability/ | icons need visible labels; only a handful of icons are universally recognised |

Note on Fluent: the Windows InfoBadge is a *transient notification* ("should
not be used as a regular icon or image"). Our `Badge` is permanent metadata
(type, download state, mute), which is Spectrum's Badge, not Fluent's
InfoBadge. Fluent is used here only for metrics, placement and the
accessibility recipe.

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Container | Fluent icon badge: 16 px ellipse; Spectrum: rounded rectangle | 22 × 22 px, `radius-s` (4 px), `Theme.badge-backdrop` (black 70 %) | ✅ (Spectrum shape; sized to hold a 16 px icon with 3 px inset) | gallery "Badges" |
| Icon | Spectrum: optional icon child; Fluent: `IconSource`, must fit | `Icon` 16 px (`Theme.icon-s`) recoloured with `tint` | ✅ | gallery "Badges" |
| Label (visible text) | Spectrum: "Badges can have a label, an icon, or both" | `Badge` is icon-only; the HDR tag is a text-only badge implemented twice as a raw `Rectangle`+`Text`, not via `Badge` | ❌ | gallery "HDR tag", `library.slint` inspector |
| Accessible name when icon-only | Spectrum: "If a visible label isn't specified, an `aria-label` must be provided" | `accessible-role: image`, `accessible-label: root.label` (mandatory prop) | ✅ | `components.slint` `Badge` |
| Semantic colour | Spectrum Badge/StatusLight: negative = red (Error, Failed); neutral = gray (Paused, Not started); notice = orange (Pending, Syncing); positive = green; info = blue; category colours for ≤ 8 categories | failed → `Theme.danger` (red) ✅; not downloaded → `Theme.text` (white) ≈ neutral ✅; video → `Theme.video` (purple), audio → `Theme.audio` (green) as category colours ✅; muted → `Theme.text` ✅ | ✅ | gallery "Badges" |
| HDR tag colour | Spectrum: orange = *notice* (Pending, Needs approval); a property/category should use a label colour, not a status colour | `Theme.warning` orange with black text for a capability tag | ❌ | gallery "HDR tag" |
| Pending / processing indicator | StatusLight: notice (Pending, Processing) | — no badge while metadata is read; the cell shows a dimmed placeholder icon and the inspector says "Reading metadata…"; a badge that flips off after a second would flicker across the grid | ➖ | gallery "Library cells: … pending" |
| Dot and numeric variants | Fluent: dot 4 px, numeric auto-width | — no counters or "new" markers exist; the drag ghost count is its own element | ➖ | |
| Visible label near the icon | NN/G: "Icon labels should be visible at all times" | no tooltip on `Badge`; the meaning is only in the inspector (Type / Status rows) once the cell is selected | ❌ | `components.slint` `Badge` has no `Tooltip` |

## States

Badges are non-interactive; states are the value variants.

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| video / audio type | Spectrum category colour | `KindBadge kind 1 / 2` | ✅ | gallery "Badges" 1–2 |
| not downloaded (cloud) | Spectrum neutral | `KindBadge cloud` white cloud | ✅ | gallery "Badges" 3 |
| failed | Spectrum negative | `KindBadge failed` red error-circle | ✅ | gallery "Badges" 4 |
| muted | — | `Badge` speaker-mute, white | ✅ | gallery "Badges" 5, "Timeline clips" 4 |
| HDR | — | orange tag, "HDR" | ✅ shown (colour gap in Anatomy) | gallery "HDR tag" |
| priority when several apply | Fluent: do not mix badge types in one view; change type without jarring size change | one slot per cell: failed > cloud > kind (a failed or undownloaded video loses its type badge); all variants are the same 22 px box | ➖ one slot keeps the thumbnail readable; type is still in the inspector and the placeholder icon | `library.slint` `KindBadge` |
| hover / pressed / focus / disabled | n/a, not interactive | — | ➖ | |
| show / hide transition | Fluent: badge appears and disappears; "smooth animation" for width | conditional instantiation, no animation | ➖ metadata badges do not toggle at runtime except cloud→ready | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| badge size | Fluent icon badge 16 px; DESIGN.md min click target n/a (not interactive) | 22 × 22 px (icon 16 + 3 px inset) | ✅ |
| icon size | Fluent: icon must fit; DESIGN.md: 16 or 20 | 16 px | ✅ |
| radius | DESIGN.md: 4 px controls | `radius-s` | ✅ |
| backdrop | Fluent preset styles ensure contrast on any background | `#000000b3` semantic token `badge-backdrop` | ✅ |
| placement | Fluent: inside the parent's bounding box, top-right corner | cell: `x = width − 22 − 4`, `y = 4`; clip: 8 px inset top-right | ✅ |
| HDR tag height | one size | inspector: `control-height` (32 px); gallery: `control-height-s` (24 px) | ❌ inconsistent |
| HDR tag width / type | — | 44 px, 11 px semibold | ➖ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Not focusable, parent is | Fluent: "The parent element … should be focusable and accessible by tab" | `CellView` is `accessible-role: button`; `ClipView` parent likewise | ✅ | — |
| Parent announces the badge | Fluent: parent uses FullDescription / ItemStatus to announce badge | `CellView.accessible-label` is the title only; the badge's own label sits on a nested `image` | ❌ | — |
| Announce status changes | StatusLight: `role="status"` when status changes at runtime | — Slint has no live-region equivalent | ➖ | — |
| Hover reveals meaning | NN/G: labels visible; DESIGN.md §2: "Icons never carry meaning alone" | no tooltip | ❌ | manual-checks "Every icon-only button shows a tooltip" covers buttons, not badges |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | Spectrum: labelled element | `image` with `accessible-label` (Badge); HDR tag is a `Text` (role text) | ✅ |
| label / tooltip | Spectrum: aria-label mandatory; NN/G: visible label | label ✅, no tooltip ❌ | ❌ |
| contrast UI ≥ 3:1 | tint on black 70 % over footage: `video #a06cf5`, `audio #2fae7a`, `danger #e5484d`, white — all ≥ 4.5:1 on black; HDR black on `#e8a838` ≈ 10:1 (estimates, not measured) | ✅ |
| meaning not by colour alone | DESIGN.md §2: no red/green-only pair | each variant has a distinct glyph | ✅ |
| focus visible | not focusable | ➖ |

## Wording

| String | Rule | Status |
|---|---|---|
| "Video", "Audio" (accessible labels) | noun, sentence case | ✅ |
| "Not downloaded" | states the fact, not the icon ("cloud") | ✅ |
| "Could not be read" | Primer: say what happened; "why/what next" lives in the inspector "Error: {}" | ✅ |
| "Muted" | sentence case | ✅ |
| "HDR" | literal, not `@tr()`; acronym identical in German | ➖ acceptable, note for the string audit |

## Gaps

1. **should** — `Badge` has no `Tooltip`, so the cloud, error and mute glyphs
   carry meaning alone on hover (NN/G, DESIGN.md §2). Fix: add
   `Tooltip { text: @markdown("\{root.label}"); }` to `Badge` (it already
   has the mandatory `label`), matching `IconButton`.
2. **should** — The HDR tag is a text badge hand-built twice
   (`library.slint` inspector at 32 px, gallery at 24 px) instead of a
   `Badge` variant; heights differ. Fix: give `Badge` an optional `text`
   (Spectrum "label" badge: auto width, 24 px tall, caption semibold) and
   use it for HDR in both places.
3. **nice** — The HDR tag uses `Theme.warning` (Spectrum notice/orange =
   pending, needs approval) for a capability that is neither a warning nor
   pending. Fix: use a category colour from the semantic set
   (`Theme.title-clip` amber is also a status-free colour) or the neutral
   `badge-backdrop` with white text; keep black-on-colour contrast ≥ 4.5:1.
4. **nice** — Screen readers get the badge only as a nested image; Fluent
   asks the focusable parent to announce it. Fix: extend
   `CellView.accessible-label` to "{title}, {kind}[, not downloaded | could
   not be read]" (and `ClipView` "…, muted") built in the view model, so
   the label is one string.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.
