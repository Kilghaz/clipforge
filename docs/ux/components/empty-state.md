# EmptyState (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `EmptyState`
Usages: library empty and search/filter no-results (`crates/app/ui/library.slint`),
timeline empty (`crates/app/ui/editor.slint`)
Gallery rows: "Section + ValueSlider (inspector form), 280 px wide;
EmptyState; DialogButtons Windows / macOS" (library variant only) in
`crates/app/ui/gallery.slint`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Primer | https://primer.style/product/ui-patterns/empty-states/ | anatomy (graphic, primary text, secondary text, primary action, secondary action, border), the three types, wording per type |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/IllustratedMessage.html | anatomy order (illustration, heading, content), centred layout, illustration `aria-hidden` when a heading exists, use for empty and no-results |
| NN/G | https://www.nngroup.com/articles/empty-state-interface-design/ | state the system status truthfully (empty vs loading vs error), teach in context, give a direct action, say how |
| Primer | https://primer.style/product/getting-started/foundations/content/ | sentence case, no exclamation marks, imperative verbs |

(No Fluent page: Windows has no empty-state control; DESIGN.md §1 assigns the
pattern to Primer and Spectrum.)

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Graphic | Primer: previews the feature; error states use reinforcing imagery, not playful art. Spectrum: illustration first, `aria-hidden` when a heading exists | `Icon` 40 px, `text-secondary`, no accessible role | ✅ | gallery "EmptyState" |
| Primary text (heading) | Primer: title explaining purpose; welcoming for unused features, factual for temporarily empty. Spectrum: `Heading` | `Text` 16 px semibold, centred, wraps | ✅ | gallery "EmptyState" |
| Secondary text | Primer: optional, brief, actionable; Spectrum: `Content` | `Text` 14 px `text-secondary`, centred, wraps; omitted when empty | ✅ | gallery "EmptyState" |
| Primary action | Primer: one encouraged button, concise descriptive copy | `ActionButton primary` bound to `action()`; `action-enabled` | ✅ | gallery "EmptyState" |
| Secondary action | Primer: optional text link *below* the primary (e.g. "Learn more") | second `ActionButton` *beside* the primary (`Add files…`) | ➖ ours is a peer action, not a "learn more" link; Slint has no link button; side-by-side is Spectrum ButtonGroup order | gallery "EmptyState" |
| Border | Primer: invisible by default | none | ✅ | |
| Drag-and-drop hint | DESIGN.md §3: "drag-and-drop as the visible alternative" | not mentioned in any of the three descriptions (library says "Add the photos…"; timeline says "drag them onto the timeline") | ❌ (library) | `library.slint` line 308 |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| unused feature (first use) | Primer type 1: welcoming, explain benefit, primary action | library empty: "Your library is empty" + Add folder… / Add files… | ✅ | gallery "EmptyState" |
| temporarily empty (depends on content) | Primer type 2: convey it is empty by nature | timeline empty: "Your slideshow is empty" + Add all from library | ✅ | none in gallery |
| no results | Spectrum: "No search results"; NN/G: state status + direct action | "Nothing matches your search" + hint, **no action button** | ❌ | none in gallery |
| error / system failure | Primer type 3; NN/G: distinguish error from empty | — no error empty state; scan/import errors go to the status bar and inspector | ➖ | |
| loading vs empty | NN/G: "Inaccurate status messages that mislead users about loading states" | library shows "Your library is empty" whenever `rows == 0 && total == 0`, also while the first scan/import is running (`status-kind` 1/2 not checked) | ❌ | `library.slint` line 308 |
| primary action disabled | Primer: push users toward alternative paths, not dead ends | timeline: `action-enabled: LibraryState.total-count > 0`; when the library is empty the button is a dead end with no explanation | ❌ | none in gallery |
| hover / pressed / focus on the action | Fluent Button (std) | `ActionButton` = std `Button` | 🔒 | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| illustration size | Spectrum: unspecified; DESIGN.md §2: icons 16 / 20 px only | 40 px literal (`Icon { size: 40px }`), no token | ❌ |
| spacing between elements | Spectrum centred stack; DESIGN.md scale 4–40 | 12 px (`space-m`) | ✅ |
| padding | DESIGN.md panel padding 16, dialog 24 | 24 px (`space-xl`) | ✅ |
| max width / line length | Fluent typography: 50–60 characters per line | timeline usage caps at 480 px; library usage stretches to the pane width (up to `library-max-width`), the component has no `max-width` | ❌ |
| type sizes | DESIGN.md: 16 title / 14 body | 16 semibold / 14 regular | ✅ |
| button size | DESIGN.md controls 32 px | `ActionButton` 32 px | ✅ |
| alignment | Spectrum: centred | `alignment: center`, texts centred | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Primary action reachable by keyboard | Fluent Button | std `Button`: Tab focus, Enter/Space | 🔒 | manual-checks "Empty library and empty timeline show … a working primary action" |
| Action opens a file dialog → ellipsis | Primer/DESIGN.md §3 | "Add folder…", "Add files…" ✅; "Add all from library" has none and opens none ✅ | ✅ | |
| Disappears as soon as content exists | NN/G: status accuracy | conditional on `rows.length == 0` / `clip-count == 0` | ✅ | |
| Drop target still works while shown | DESIGN.md §3 | drop overlay is window-level (`Shell.drop-hover`), independent of the empty state | ✅ | manual-checks M1 |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| illustration hidden from AT when a heading exists | Spectrum | `Icon` (Image) has no accessible role | ✅ |
| heading semantics | Spectrum `Heading` | `Text` (role text); Slint has no heading role | ➖ |
| contrast | title `text` ≫ 4.5:1; description `text-secondary` ≈ 6.9:1 on window background (estimate) | ✅ |
| focus visible on action | std Button focus ring | 🔒 |

## Wording

Primer content rules: sentence case; no exclamation marks; imperative verb
on actions; explain purpose (unused) or nature (temporary); say how, avoid
jargon. DESIGN.md §3: use the user's words, drag-and-drop as visible
alternative. German ~30 % longer.

| Usage | String | Check | Status |
|---|---|---|---|
| library empty (`library.slint` 308) | "Your library is empty" | welcoming, factual, sentence case, no punctuation | ✅ |
| | "Add the photos, videos and music for your slideshow. Files stay where they are; nothing is copied." | explains purpose and benefit; two full sentences with punctuation; but does not mention that files can be dropped here | ❌ (drag-and-drop) |
| | "Add folder…" / "Add files…" | imperative, ellipsis (file dialog) | ✅ |
| no results (`library.slint` 317) | "Nothing matches your search" | shown also when only the type filter excludes everything (search text empty) → inaccurate | ❌ |
| | "Try another word or set the type filter to \"All\"." | says how ✅; straight quotes in EN, „…“ in DE ✅; still no button to do it | ❌ (no action) |
| timeline empty (`editor.slint` 479) | "Your slideshow is empty" | temporary-empty phrasing, sentence case | ✅ |
| | "Double-click items in the library or drag them onto the timeline below. Everything you change can be undone." | says how, teaches undo ✅; "items" is our word, DESIGN.md prefers photo/video/clip | ➖ "items" is used consistently app-wide ("12 items") |
| | "Add all from library" | imperative ✅ | ✅ |
| German terminology | `.po`: "Timeline" in 4 strings ("Zur Timeline hinzufügen", "Aus Timeline entfernen", …) vs "Zeitleiste" in 4 others (this description, "{} aus der Zeitleiste entfernen") | Primer: one term per concept | ❌ |

## Gaps

1. **should** — No-results state has no action (NN/G, Primer "push users
   toward alternative completion paths"). Fix: `action-text: @tr("Clear
   search and filter")` calling a `LibraryState.clear-filters()` that resets
   `search-text` and `filter-index`; add a German entry.
2. **should** — "Nothing matches your search" is shown when the search text
   is empty and only the type filter hides everything. Fix: branch the title
   on `LibraryState.search-text == ""`: "No videos / audio in your library"
   (per filter) vs "Nothing matches your search"; keep one description.
3. **should** — Library empty state is displayed while the first scan or
   import is still running (`status-kind` 1/2), telling the user the library
   is empty when it is loading (NN/G). Fix: gate the condition on
   `LibraryState.status-kind == 0 || == 3`, and while scanning show a
   `ProgressIndicator { indeterminate }` + "Scanning…" instead.
4. **should** — Library empty description does not offer drag-and-drop as
   the visible alternative (DESIGN.md §3). Fix: append "You can also drop
   files or folders anywhere in this window." and translate.
5. **should** — German uses both "Timeline" and "Zeitleiste" for the same
   concept (Primer terminology consistency). Fix: pick one (DESIGN.md
   glossary) and update the `.po`; the empty-state description is one of
   the eight affected strings.
6. **nice** — Timeline empty state disables "Add all from library" when the
   library is empty with no explanation (dead end). Fix: when
   `LibraryState.total-count == 0` swap `action-text` to "Add folder…"
   (calls `LibraryState.add-folder()`) and change the description to point
   at the library first.
7. **nice** — Gallery shows only the library variant. Missing: no-results
   (no button), timeline (single action, disabled action), title-only.
   Fix: add three `EmptyState` instances to the gallery row.
8. **nice** — Illustration is a 40 px literal outside the icon tokens
   (DESIGN.md §2 lists 16/20). Fix: add `Theme.icon-xl: 40px` (and
   `icon-l: 28px` for the cell placeholder) and use them.
9. **nice** — Component has no `max-width`; in a wide library pane the
   description exceeds Fluent's 50–60 characters per line. Fix:
   `max-width: 480px` inside `EmptyState` (the timeline usage already does
   this at the call site) and centre it in the parent.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.
