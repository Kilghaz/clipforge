# Design and usability guide

How ClipForge should look, behave and be checked. This is the operating
manual for anyone (human or agent) building UI; the decision behind it is
ADR-0009. Read the relevant parts **before** designing a feature and run the
checklist at the end **after** implementing it.

## 1. Sources and their roles

ClipForge does not invent its own design language. Four established sources
are combined, each with a fixed responsibility. When two disagree, the one
higher in this list wins for the question it owns.

| Role | Source | Use it for |
|---|---|---|
| **Platform behaviour** | [Windows app design / Fluent 2](https://learn.microsoft.com/en-us/windows/apps/design/) (primary target); [Apple HIG for macOS](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos) (secondary) | Menus, dialogs and button order, keyboard conventions, drag and drop, selection, context menus, window and panel behaviour, accessibility expectations. |
| **Editor components and visuals** | [Adobe Spectrum](https://spectrum.adobe.com/) | Anything "creative tool": panels, sliders, dense layouts, media grids, colour on dark surfaces, spacing and sizing scale, motion, iconography rules, inline alerts, toasts, progress. |
| **Workflow patterns and content** | [GitHub Primer](https://primer.style/product/) (Patterns, Foundations → Content, Accessibility) | Saving, loading, empty states, notification messaging, progressive disclosure, forms, degraded experiences, feature onboarding, wording of labels and errors. **Not** its colours, typography or components. |
| **Usability rules** | [Nielsen Norman Group](https://www.nngroup.com/) | The 10 usability heuristics and the topic articles; see §5. |

Rationale in short: Slint renders native `fluent` (Windows) and `cupertino`
(macOS) widgets, so behaviour must follow the platform. Spectrum is the only
mature system written for editing tools. Primer's patterns and content
guidance are excellent but its visual system is GitHub's web brand and does
not belong in a desktop app.

### Quick links to look up per topic

| Topic | Read first |
|---|---|
| Drag and drop | [NN/G drag and drop](https://www.nngroup.com/articles/drag-drop/), Fluent "Drag and drop", Spectrum "Drag and drop" |
| Multi-select, bulk edits | [NN/G bulk actions](https://www.nngroup.com/videos/bulk-actions-design-guidelines/), Spectrum "Action bar" |
| Direct manipulation, undo | [NN/G direct manipulation](https://www.nngroup.com/articles/direct-manipulation/), Primer "Saving" |
| Long-running work, progress | [NN/G visibility of system status](https://www.nngroup.com/articles/visibility-system-status/), Primer "Loading", Spectrum "Progress bar / circle" |
| Errors, confirmations | Primer "Notification messaging", Spectrum "Alert dialog", "Inline alert", "Toast" |
| Empty library / timeline | Primer "Empty states", Spectrum "Illustrated message" |
| Dialogs (export, settings) | Fluent "Dialogs and flyouts", Spectrum "Dialog", Primer "Forms" |
| Menus, shortcuts | Fluent "Menus and context menus", Apple HIG "Menus", `docs/ux/keyboard.md` |
| Timeline, scrubbing, playback | Spectrum "Slider", Apple HIG "Playing video", NN/G direct manipulation |
| Wording | Primer "Content" guidelines (UI text, terminology, capitalisation) |

## 2. Visual foundations

- **Dark only.** The app forces `Palette.color-scheme = ColorScheme.dark` and
  ships no light theme (ADR-0009). Footage and photos are judged against dark
  neutral surfaces, as in Spectrum's "dark" theme.
- **Colours come from `Palette`.** Custom components use the Slint `Palette`
  globals (`background`, `foreground`, `control-background`, `accent-*`,
  `selection-*`, `border`, ...) so the fluent and cupertino styles stay
  consistent. Hard-coded hex values are allowed only for the semantic
  tokens below and for pixel data (thumbnails, previews, waveforms).
- **Surfaces are layered**, Spectrum style: window background (lowest) →
  panel / pane → card / clip → overlay / popover. Each layer is one step
  lighter than the one beneath; use `Palette.alternate-background` for panes
  and a 1 px `Palette.border` where two panes meet.
- **Spacing scale** (Spectrum): 4, 8, 12, 16, 24, 32, 40 px. Nothing else.
  Component internal padding 8 or 12, gap between related controls 8, between
  groups 16, panel padding 16, dialog padding 24.
- **Sizing:** controls 32 px tall (Spectrum size M) in toolbars and dialogs,
  24 px (size S) inside dense areas such as clip chips and timeline rulers.
  Minimum click target 24 × 24 px, minimum touch-safe 32 × 32. Text buttons
  are at least 96 px wide in panes and 120 px in dialogs (Fluent asks 120
  everywhere; panes as narrow as 280 px cannot afford it).
- **Corner radius:** 4 px for controls, 6 px for cards and clips, 8 px for
  dialogs and popovers.
- **Typography:** body is 14 px. Every window sets
  `default-font-size: Theme.font-body`, so std widgets (Button behind
  `ActionButton`, ComboBox, Switch, LineEdit) use the same size as our own
  controls; without it Slint falls back to 12 px. Scale: 11 (caption / timecode), 12 (secondary), 14
  (body), 16 (section title), 20 (dialog title). No other sizes. Accepted
  deviation from Fluent (12 px minimum, 14 semibold section headers): the
  editor follows Spectrum's denser scale; captions are never the only
  carrier of essential information. Numbers in
  timecodes and counters use tabular figures where the font allows.
- **Semantic tokens** (defined once in `ui/theme.slint`, never inline):
  `photo`, `video`, `audio`, `title` clip colours; `warning` and `danger`;
  `drop-target` highlight; `playhead`. Chosen for contrast ≥ 3:1 against the
  clip surface and distinguishable for deuteranopia (no red/green pair as the
  only difference).
- **Icons:** Fluent UI System Icons (MIT), regular weight at 16 and 20 px,
  filled weight only for the "on" state of a toggle. Icons never carry meaning
  alone; every icon-only button has a tooltip and an accessible label. No
  emoji as icons.
- **Motion:** 100–150 ms ease-out for state changes, 200–250 ms for panels
  appearing. Motion communicates cause and effect (where a clip went, where
  a panel came from); no decorative animation. Respect reduced-motion when
  the platform exposes it.

## 3. Behaviour foundations

- **Direct manipulation first.** Whatever the user sees can be grabbed,
  moved, resized or deleted in place, with a live preview during the gesture
  and a single undo step afterwards.
- **Undo everything.** Every edit is a `Command` with an inverse (CLAUDE.md
  rule 2). Destructive actions that cannot be undone (deleting files on disk,
  overwriting an export) get a confirmation; everything else gets undo, not a
  dialog.
- **Selection model** follows the platform: click selects, Ctrl/Cmd-click
  toggles, Shift-click ranges, marquee on empty space, Ctrl/Cmd+A selects
  all, Escape clears. Selected items show `Palette.selection-background`.
- **Bulk actions** operate on the selection, are one command / one undo
  step, and show the affected count in the action ("Remove 12 clips").
- **Feedback within 100 ms** for every action. Work longer than 1 s shows
  progress (determinate when the total is known), longer than 10 s can be
  cancelled and lets the user continue working (CLAUDE.md rule 4).
- **Dialogs are rare.** Prefer inline editing, inspector panels and toasts.
  A dialog is used for export settings, the settings screen and irreversible
  confirmations. Button order follows the platform: Windows `[OK] [Cancel]`,
  macOS `[Cancel] [OK]`; the primary action is named after the verb
  ("Export", not "OK").
- **Empty states explain and offer the next step**: what this area is, why
  it is empty, one primary action (e.g. "Add folder…"), and drag-and-drop
  as the visible alternative.
- **Errors say what happened, why, and what to do next**, in the user's
  language, without codes in the primary text. Never lose user work on an
  error.
- **Keyboard:** every action reachable by mouse is reachable by keyboard;
  shortcuts are listed in `docs/ux/keyboard.md` and in the menus. Focus is
  always visible.
- **Text:** sentence case for everything, ending punctuation only for full
  sentences, ellipsis on actions that open a dialog ("Export…"). All strings
  through `@tr()` with a German entry (CLAUDE.md rule 7).

## 4. Component policy

1. Use a Slint `std-widgets` component if one exists (Button, CheckBox,
   ComboBox, LineEdit, ListView, ScrollView, Slider, SpinBox, Switch,
   TabWidget, ProgressIndicator, StandardButton, …). They follow the platform
   style for free.
2. If none fits, look for the component in Spectrum, then in Fluent 2, and
   build a custom Slint component in `ui/components/` that copies its
   **states** (default, hover, down, focus, disabled, selected, drag-over)
   and **behaviour**, using `Palette` colours and the tokens in §2.
3. Only then design something new, and document it in this file under
   "ClipForge components" with the states it supports.

Every component, new or changed, gets a row in `crates/app/ui/gallery.slint`
showing **all its states** (default, hover where simulable, pressed, disabled,
selected, primary, empty content). Render it with
`cargo run -p clipforge-app --bin screenshot -- gallery` (2× scale) and look
before and after. The gallery is our storybook: it is the only place where a
component is seen in isolation with a centre guide, so alignment and sizing
faults show up there first.

Enforced by tests (`crates/app/tests/ui_tokens.rs`): no hex or named colours,
no literal `border-radius`/`font-size` px outside `theme.slint`, and no std
`Button` carrying an icon without text (it cannot centre the icon; use
`IconButton`). Radii of std widgets are owned by the style (fluent 4 px, the
same as `Theme.radius-s`; cupertino rounds slightly more) and are accepted.

### ClipForge components

All live in `crates/app/ui/components.slint` unless noted; tokens in
`crates/app/ui/theme.slint` (`Theme`, `Icons`).

| Component | Based on | Notes |
|---|---|---|
| `Icon` | Fluent iconography | SVG recoloured via `colorize`; 16 or 20 px. |
| `IconButton` | Fluent "Button", Spectrum "Action button" | std `Button` with icon + mandatory `Tooltip` and accessible label. |
| `ActionButton` | Fluent "Button" | std `Button` pinned to `control-height` (a bare `Button` stretches to its layout cell and breaks alignment). Use it for every text button. |
| `SearchField` | Fluent "TextBox" + Spectrum "Search field" | Magnifier left, clear button when text is present, Escape clears, accent focus line. Replaces the bare `LineEdit` for search. |
| `ChoiceGroup` | Fluent "RadioButtons", Spectrum "ActionGroup" (single select) | Labelled set of 2–5 options: one tab stop, arrow keys, accessible group name. Replaces ComboBoxes with few static options (framing, orientation, export resolution/quality, presets). |
| `Title`, `SecondaryText` | Spectrum typography | Complete the text roles (16 semibold, 12 regular) so screens never set a font size directly. |
| `TimelineStrip` (`editor.slint`) | Premiere conventions, NN/G direct manipulation | Ruler, clips, playhead with grab head, drop and trim markers; exposes its on-screen rect for drops. |
| `ChoiceButton` | Spectrum "Action group" | One option of a small set (duration presets); own selected/hover/pressed/disabled states because the std checked style differs per platform. |
| `Bar` | Fluent "CommandBar" | Fixed-height bar whose children are exactly `control-height` tall and vertically centred. Use it for every toolbar, filter and action row. |
| `Section` | Spectrum "Form" (vertical labels) | Label above controls, 8 px inside, 24 px between sections. Use it for inspectors and dialogs. |
| `SectionLabel`, `Caption`, `BodyText` | Spectrum typography | The three text roles; no other sizes in screens. |
| `Divider`, `VDivider` | Fluent separators | 1 px `Palette.border`. |
| `Badge` | Spectrum "Badge" | Icon or short-text status over thumbnails/clips, tinted by semantic colour, always with a tooltip naming its meaning. |
| `EmptyState` | Primer "Empty states", Spectrum "Illustrated message" | Icon, title, description, primary + secondary action. |
| `DialogFrame`, `DialogButtons` | Fluent "Dialog", Apple HIG "Alerts" | Scrim, Escape to dismiss, platform button order via `Shell.macos`. |
| Library grid cell (`library.slint`) | Spectrum "Card" (quiet) + Fluent "GridView" selection | Thumbnail, type badge, hover tint, accent selection ring, drag source. Geometry mirrored in `library_view.rs`. |
| Timeline clip (`editor.slint`) | Spectrum "Card" + custom | Type colour stripe, thumbnail, solid label footer, transition overlay with a high-contrast end edge, mute / motion badge at the left after the overlay, trim handles shown on hover. |
| Timeline ruler (`editor.slint`) | Premiere / Spectrum "Slider" ticks | One tick per second, labels thinned by zoom level, playhead with grab head. |
| `ValueSlider` (`editor.slint`) | Spectrum "Slider" | Slider with live value read-out and accessible label. |
| Panel divider (`main.slint`) | Fluent "SplitView" | 1 px line, 8 px grab area, accent on hover, double-click resets. |
| `TextLane` (`editor.slint`) | Premiere text track, NN/G direct manipulation | Texts on the strip's time scale above the clips; drag to move, edges to trim, overlaps in up to three rows; double-click to add. |
| `TextOverlay` (`editor.slint`) | Keynote / Canva on-canvas text, NN/G direct manipulation | Selection box with corner (size) and side (width) handles over the preview, centre snap guides, inline editor with the text's own font. |
| `FontPicker` | Spectrum "ComboBox" | Font field with type-to-filter over all installed and bundled fonts; the list previews each family in its own font. |
| `IconChoiceGroup` | Spectrum "ActionGroup" (single selection) | Icon-only single choice (text alignment) with tooltips; one tab stop, arrow keys. |
| `MusicLane` (`editor.slint`) | Premiere / Clipchamp audio track, NN/G direct manipulation | 40 px lane under the clips: song blocks on the strip's time scale, looped repeats dimmed, fade-out ramp, empty state with "Add music…"; click / Enter selects the music. |
| `SongRow` | Fluent list view item | Song name and length with quiet move up / down / remove buttons (Music inspector). |
| `SwatchGroup` | Spectrum "ColorSwatchPicker" | Single choice from named colour swatches; one tab stop, arrow keys, names as tooltips. Title card backgrounds. |
| Drop overlay / drag ghost | Fluent drag-and-drop visuals | Tinted zone + border while hovering; ghost shows icon and count, accent when droppable. |

(Add a row whenever a custom component lands.)

## 4b. Spec cards and skills

Each component has a spec card in `docs/ux/components/<name>.md` (template:
`TEMPLATE.md`), written from the fetched reference pages, listing every
element, state, metric and behaviour rule the references define, with a
status and evidence per row. The card exists before the component is used in
a screen. Four project skills run the ritual:

| Skill | When | Produces |
|---|---|---|
| `/ux-prepare <feature>` | before coding a visible feature | task statement, sources, behaviour decisions, components to reuse |
| `/ux-component-spec <component>` | component created or touched | spec card + gap list |
| `/ux-review` | after any UI change | findings with severity from renders, cards, heuristics |
| `/ux-feature-audit <area>` | per feature area, periodically | `docs/ux/AUDIT-<date>.md` findings |

Tests back this up: `ui_tokens.rs` (no literals outside the theme) and
`gallery_coverage.rs` (every exported component has a gallery row; cards
point at real components).

## 5. Usability checks

### Before implementing

1. Name the user task in one sentence ("The user wants to reorder ten clips
   they just imported").
2. Look up the topic in the quick-links table above; read the NN/G article
   and the Spectrum / Fluent / Primer page for the pattern.
3. Write down the behaviour decisions the sources imply (selection, undo,
   feedback, keyboard, empty and error states) in the PR / commit or the
   task notes, before writing code. Tests for view-model behaviour follow
   from this list.

### After implementing

Run through the [10 usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/)
against the feature and tick each line:

- [ ] **Visibility of system status:** every action gives feedback within
      100 ms; long work shows progress and can be cancelled.
- [ ] **Match with the real world:** labels use the user's words (photo,
      video, clip, slideshow), not ours (asset, node, entity).
- [ ] **User control and freedom:** undo works for every change; Escape
      cancels every gesture and dialog; nothing is lost on error.
- [ ] **Consistency and standards:** platform conventions (§3), spacing and
      sizing (§2), and existing ClipForge components were reused; no new
      colour or size was introduced.
- [ ] **Error prevention:** irreversible actions confirm; invalid inputs are
      prevented or flagged inline, not after submission.
- [ ] **Recognition over recall:** options are visible (menus, tooltips,
      shortcuts shown), not memorised.
- [ ] **Flexibility and efficiency:** keyboard path exists, bulk operation
      exists where a single one exists, shortcut documented.
- [ ] **Aesthetic and minimalist design:** nothing on screen that the task
      does not need; secondary information is a step away, not hidden.
- [ ] **Help users with errors:** error text says what, why and what next in
      plain language, translated.
- [ ] **Help and documentation:** empty states and tooltips teach the
      feature; `docs/ux/keyboard.md` and `manual-checks.md` are updated.

Plus the visual pass. Render the UI offscreen first; it needs no window
and no screen-recording permission:

```
cargo run -p clipforge-app --bin screenshot          # all scenes
cargo run -p clipforge-app --bin screenshot -- populated narrow
```

Scenes land in `target/screenshots/` (`empty`, `populated`, `export`,
`settings`, `narrow` = 900 × 560). Look at every one that the change touches;
add a scene to `crates/app/src/bin/screenshot.rs` when a new state appears.

- [ ] Screenshot on the fluent style (and cupertino if available) and compare
      against the Spectrum / Fluent reference for the component: states,
      spacing, alignment, contrast ≥ 4.5:1 for text and ≥ 3:1 for UI
      elements, focus ring visible, icons at 16/20 px, no emoji.
- [ ] Resize the window to the minimum supported size and to a 4K display at
      200 % scale; nothing overlaps, truncates without ellipsis, or drifts.
- [ ] German strings fit (they run ~30 % longer than English).

Anything that fails is fixed before the feature is called done.
