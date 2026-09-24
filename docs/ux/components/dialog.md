# DialogFrame and DialogButtons (spec card)

Status: audited 2026-09-24
Implementation: `crates/app/ui/components.slint` → `DialogFrame`, `DialogButtons`
Usages: Settings (`crates/app/ui/main.slint`, content in `settings.slint`),
`ExportDialog` (`crates/app/ui/editor.slint`)
Gallery rows: "DialogFrame (scaled preview, 480 × 200) with DialogButtons"
and "… DialogButtons Windows / macOS" in `crates/app/ui/gallery.slint`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/dialogs | title/content rules, CloseButton (safe action, rightmost, Esc), PrimaryButton leftmost, DefaultButton (accent, Enter, initial focus), one dialog per window, confirmation button order |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Dialog.html | anatomy (Heading, Header, Divider, Content, Footer, ButtonGroup), sizes S/M/L, `isDismissable` close button, `autoFocus`, role dialog/alertdialog, `aria-labelledby` heading |
| Apple HIG | https://developer.apple.com/design/human-interface-guidelines/alerts (HTML page is JS-rendered and empty; read the same content via https://developer.apple.com/tutorials/data/design/human-interface-guidelines/alerts.json) | default button trailing, Cancel leading, Return/Escape, verbs not OK, destructive style, "Cancel" wording |
| NN/G | https://www.nngroup.com/articles/modal-nonmodal-dialog/ | when a modal is justified (required info, prevent loss), cost of interruption |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Scrim / underlay | Fluent: dialog blocks the window; Spectrum: underlay | full-window `Rectangle` `Theme.scrim` (black 65 %) with a swallowing `TouchArea` | ✅ | gallery "DialogFrame" |
| Surface | Fluent ContentDialog; DESIGN.md: radius 8, dialog padding 24 | `surface-pane`, 1 px `border`, `radius-l`, padding 24, spacing 16 | ✅ | gallery "DialogFrame" |
| Title (Heading) | Fluent: optional, short, relates to the buttons; Spectrum: Heading required, labels the dialog | `title` 20 px semibold (`font-dialog`) | ✅ | gallery "DialogFrame" |
| Header / Divider / Footer | Spectrum: optional | — no dialog needs them yet | ➖ | |
| Content | Fluent: required; Spectrum: Content required | `@children` in the padded column | ✅ | Settings, Export |
| Button group | Fluent: CloseButton required + up to two "do it"; Spectrum: ButtonGroup | `DialogButtons`: one primary + optional secondary | ✅ | gallery "DialogButtons Windows / macOS" |
| Third button | Fluent: SecondaryButton optional, "used sparingly" | — none needed | ➖ | |
| Close (X) affordance | Spectrum: only for `isDismissable` dialogs, *instead of* buttons; Fluent: the CloseButton in the row | button row, no X | ✅ (Fluent pattern) | |
| Default button visual | Fluent: default gets the accent treatment; Apple: default on trailing side | `primary: true` accent button | ✅ | gallery |
| Destructive button style | Apple: destructive style when the action was not deliberately chosen; Fluent: none | — no destructive dialog exists; DESIGN.md §3 confirms only irreversible actions | ➖ (add a `destructive` flag when the first one lands) | |
| Icon (caution) | Apple macOS: sparingly | — | ➖ | |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| open | Fluent: modal, blocks window | conditional `if Shell.settings-open` / `if EditorState.export-open` | ✅ | manual-checks "Design guide pass" |
| appearing / dismissing motion | DESIGN.md §2: 200–250 ms for panels appearing | none; pops in and out | ❌ | |
| busy (export running) | Primer saving/loading; NN/G status | `ProgressIndicator` + "Exporting… {}%"; controls disabled; primary hidden; secondary becomes "Cancel" | ✅ | none in gallery |
| success / failure result | Primer: say what happened | icon + "Saved to {}" / "Export failed: {}" | ✅ | none in gallery |
| single-button (informational) | Fluent: one safe button; Apple: "Done", not "Cancel" | Settings: "Close" as accent primary only | ✅ | none in gallery |
| primary disabled | Fluent | `primary-enabled` prop, unused by callers | ✅ prop, no gallery row | |
| focus (keyboard) | Fluent: default button focused unless content is focusable | `FocusScope.init => focus()` puts focus on the invisible scope, no control shows a ring | ❌ | |
| two dialogs at once | Fluent: "only one ContentDialog open per window" | Settings (gear button, View menu) can open over an open Export dialog; nothing prevents it | ❌ | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| width | Spectrum: sizes S / M / L (values not on the fetched page); Fluent: not stated on the fetched page | fixed `dialog-width` 480 (Export) / 520 (Settings) | ✅ within a 900 px minimum window |
| height | Spectrum/Fluent: sized by content | fixed `dialog-height` 380 / 320; content that grows (German status line, wrapped error) has no room | ❌ |
| padding | DESIGN.md: dialog padding 24 | 24 | ✅ |
| title size | Fluent Subtitle 20/28 semibold | 20 semibold | ✅ |
| content spacing | DESIGN.md groups 16 | 16 | ✅ |
| button spacing | Fluent: 8 epx between buttons | 8 | ✅ |
| button height / min width | DESIGN.md controls 32 | `ActionButton` 32 × ≥ 80 | ✅ |
| radius | DESIGN.md: 8 for dialogs | `radius-l` | ✅ |
| scrim | — | black 65 % | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence (test / manual check) |
|---|---|---|---|---|
| Button order Windows | Fluent: "do it" leftmost, safe/close rightmost | `!macos`: primary then secondary | ✅ | gallery "DialogButtons Windows" |
| Button order macOS | Apple: default trailing, Cancel leading | `macos`: secondary then primary | ✅ | gallery "DialogButtons macOS" |
| Order switched per platform | DESIGN.md §3 | `macos: Shell.macos` passed by both callers; default `false` if a caller forgets | ✅ (fragile, see gaps) | `main.slint`, `editor.slint` |
| Safe, non-destructive button always present | Fluent: "All dialogs should contain at least one safe action button" | Settings "Close"; Export "Close" / "Cancel" (during export) | ✅ | |
| Escape = safe action | Fluent: Esc triggers CloseButton; Apple: Esc cancels | `Key.Escape` → `dismissed()`; Settings closes; Export closes unless exporting | ✅ | keyboard.md "Esc"; manual-checks |
| Escape while exporting | Fluent: Esc == CloseButton, whose text is "Cancel" at that moment | ignored (`export-status != 1` guard) although the visible safe button says "Cancel" | ➖ deliberate: avoids cancelling a long export by a stray Esc (keyboard.md documents it) | keyboard.md |
| Enter = default button | Fluent: DefaultButton "will respond to the ENTER key automatically"; Apple: Return activates default | no Enter handling anywhere; the accent button is default only visually | ❌ | — |
| Initial focus | Fluent: default button receives focus unless the content has focusable UI; Spectrum: `autoFocus` | focus goes to the wrapper `FocusScope`; no control is focused | ❌ | — |
| Focus stays inside (trap) | modal semantics (Fluent blocks the window) | Tab can move past the last dialog control into the page beneath (Slint has no focus trap; the scrim only blocks the pointer) | ❌ | — |
| Focus returns to opener on close | platform modal behaviour | not handled; the conditional element is destroyed | ❌ (verify manually) | — |
| Click on scrim | Fluent ContentDialog: no light-dismiss; Spectrum: only when dismissable | swallowed, dialog stays (comment: avoid accidental loss of input) | ✅ | |
| Modal is justified | NN/G: required information / prevent loss | Export needs options before an irreversible file write; Settings is a small preference set | ✅ | DESIGN.md §3 "Dialogs are rare" |
| Contextual validation not in dialogs | Fluent: inline errors on the canvas | export failure is shown inline inside the dialog with icon + text | ✅ | |
| Only one dialog per window | Fluent | not enforced | ❌ | |
| Destructive confirmation | DESIGN.md §3: confirm only irreversible actions; Apple: include Cancel with destructive | export overwrite is confirmed by the OS save dialog (`rfd`); "Remove from library" and "Remove from timeline" are not confirmed (undo/redo or file stays on disk) | ✅ | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | Spectrum: `dialog` / `alertdialog` | — Slint has no dialog role | ➖ |
| label | Spectrum: `aria-labelledby` the heading | title is a plain `Text`; no association possible in Slint | ➖ |
| contrast | title/body `Theme.text` on `surface-pane` ≫ 4.5:1; scrim dims the page to ≈ 35 % | ✅ |
| focus visible | std Button ring / our ring | nothing focused at open (see gaps) | ❌ |
| keyboard-only completion | every control reachable, Esc closes | Enter missing (gap 1) | ❌ |

## Wording

Fluent: title short, relates to buttons, no repetition in content; button
text = specific response, concise. Apple: verbs, one or two words; "Cancel"
for cancelling; avoid "OK". Primer/DESIGN.md: sentence case, ellipsis on
actions opening a dialog.

| String | Check | Status |
|---|---|---|
| "Export video" (title) | short, sentence case, relates to "Export…" | ✅ |
| "Settings" (title) | ✅ | ✅ |
| "Export…" (primary) | verb; ellipsis correct because `export-start` opens the OS save dialog | ✅ |
| "Close" (Settings primary, Export idle/done secondary) | Fluent safe action; Apple "Done"-style for informational | ✅ |
| "Cancel" (Export while running) | Apple: "Always use Cancel to title a button that cancels the alert's action" | ✅ |
| "Resolution", "Quality" (side labels) | ≤ 3 words | ✅ |
| "Optimize for YouTube" | imperative, sentence case | ✅ |
| "HDR (available once HDR sources are supported)" | disabled option explains itself; long in German (55 chars) but fits 480 px | ✅ |
| "Exporting… {}%", "Saved to {}", "Export failed: {}" | what happened; failure text may include a raw message | ✅ |
| "Export Video…" (menu item that opens the dialog, `.po` line 168) | Title Case, violates sentence case | ❌ |
| "Interface language", "Changes apply immediately." (Settings) | ✅ | ✅ |

## Gaps

1. **should** — Enter does not activate the default (accent) button
   (Fluent DefaultButton, Apple Return). Fix: in `DialogFrame`'s
   `FocusScope.key-pressed` handle `Key.Return` by emitting a new
   `callback accepted()`; `DialogButtons` (or the caller) wires it to
   `primary()` when `primary-visible && primary-enabled` and no `TextInput`
   has focus.
2. **should** — Initial focus lands on the invisible `FocusScope`, so a
   keyboard user sees no focus ring and must Tab blindly. Fix: after
   `init`, focus the first focusable content control if any, else the
   primary button (Fluent rule); expose `forward-focus` on `DialogFrame`
   so callers can pick.
3. **should** — Focus is not trapped in the modal; Tab walks into the page
   underneath while the pointer is blocked. Fix: wrap the dialog content in
   a `FocusScope` that catches `Key.Tab` / `Shift+Tab` at the ends and
   cycles, or disable the page (`enabled: !Shell.settings-open && !EditorState.export-open`)
   while a dialog is open; verify in manual-checks.
4. **should** — `dialog-height` is fixed; content does not size the dialog
   (Spectrum/Fluent size to content). A wrapped German failure message in
   the Export dialog has no room. Fix: drop `dialog-height`, let the
   `VerticalLayout` size the surface with `min-height` and
   `max-height: root.height - 2 * space-xl`; keep the `vertical-stretch`
   spacer optional.
5. **nice** — Two dialogs can be open at once (Settings over Export);
   Fluent allows one per window. Fix: the gear/View menu handler ignores
   `settings-open = true` while `EditorState.export-open`, or `DialogFrame`
   is hoisted to one window-level host with a single `open-dialog` enum.
6. **nice** — No open/close motion (DESIGN.md §2: 200–250 ms for panels).
   Fix: animate scrim `opacity` and surface `opacity`/`y` with
   `Theme.motion-panel` ease-out on appearance; no motion on dismiss is
   acceptable.
7. **nice** — Focus restore to the opener after close is unverified. Fix:
   add a manual check ("close Settings with Esc, press Space: the gear
   button re-activates") and, if it fails, call `focus()` on the opener in
   the `dismissed` handler.
8. **nice** — `DialogButtons.macos` defaults to `false`; a caller that omits
   it ships Windows order on macOS. Fix: add a `gallery_coverage`-style
   test asserting every `DialogButtons {` in `ui/` sets `macos: Shell.macos`,
   or move the flag into a `Theme.macos` token that the component reads.
9. **nice** — Gallery shows only the two-button idle state. Missing:
   single-button (Settings), disabled primary, busy row with progress and
   "Cancel". Fix: add three `DialogButtons`/`DialogFrame` instances.
10. **nice** — The menu item that opens the dialog, "Export Video…", is in
    Title Case (Primer/DESIGN.md: sentence case). Fix: "Export video…" in
    the `MenuItem` in `main.slint` (line 53) and the matching `.po` entry.

Status legend: ✅ matches · ❌ gap · ➖ deliberately omitted (reason given) ·
🔒 owned by the Slint style, accepted.

## Resolution 2026-09-24

- Fixed: Enter triggers the dialog's primary action (`accepted` callback); dialogs size to content (`dialog-height` 0); Settings cannot open over a running export; "Export video…" sentence-cased; gallery rows for single-button and disabled-primary.
- Deferred: focus trap and initial focus → AUDIT-2026-09 #13.
- Open (nice): open motion, `macos` default guard.
