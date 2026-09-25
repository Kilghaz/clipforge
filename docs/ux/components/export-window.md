# ExportWindow (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/export_window.slint` → `ExportWindow`, state in `ExportState`
Screenshot scenes: `export` (Advanced open), `export-running`, `export-done`,
`export-small` (scrolling); toolbar: `export-background`

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| NN/G | https://www.nngroup.com/articles/modal-nonmodal-dialog/ | non-modal windows for work the user keeps next to the main task; modals interrupt |
| Fluent 2 / Windows | https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/dialogs | button row, safe button, default button, Esc |
| Apple HIG | https://developer.apple.com/design/human-interface-guidelines/windows | panels / auxiliary windows; Cmd+W closes a window |
| NN/G | https://www.nngroup.com/articles/progress-indicators/ | percent done, approximate time, stop |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| title bar | OS window, title names the task | "Export video" / "Video exportieren", app icon | ✅ | app |
| body | form content, scrolls when taller | std `ScrollView`, scrollbar only when needed, 12 px gutter | ✅ | `export-small` |
| button row | fixed at the bottom | `DialogButtons`, platform order | ✅ | all export scenes |
| hairline | more content below | 1 px above the buttons while scrolled up | ✅ | `export-small` |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| idle | | "Export…" (default) + "Close" | ✅ | `export` |
| running | percent, time left, stop | progress, "about 3 min left", controls disabled; "Close" (hides, export continues) + "Cancel export" | ✅ | `export-running` |
| done | say what happened | result row with "Show in Finder / Explorer"; "Close" default, "Export again…" | ✅ | `export-done` |
| failed / warning | | error / warning icon with the message | ✅ | code |
| hidden while running | status stays visible | toolbar `ExportStatusButton`; click brings the window back | ✅ | `export-background` |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| width | | 520 preferred, 460 min, resizable | ✅ |
| height | sized by content | fitted to the content when opened and when Advanced toggles, 320..760; content scrolls beyond | ✅ |
| padding | DESIGN.md dialog padding 24 | 24 (right: 12 + 12 gutter) | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| non-modal | NN/G | the editor stays usable; project copied at export start | ✅ | manual-checks M6 |
| one instance | | "Export…" / menu / toolbar status bring the same window back | ✅ | `ui_interaction::the_toolbar_status_button_brings_the_export_window_back` |
| Esc, Cmd/Ctrl+W close | Fluent Esc, Apple Cmd+W | close (hide); never cancels an export | ✅ | `ui_interaction::closing_the_export_window_never_cancels_and_enter_after_success_closes` |
| title-bar close | | hides; export continues | ✅ | code (`on_close_requested`) |
| Enter | default button | Export…, or Close right after success | ✅ | same test |
| main window closes | | export window closes too | ✅ | code |
| focus return | | not handled (OS decides) | ➖ | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| window title | | "Export video" | ✅ |
| controls labelled | | accessible labels on pickers, switch texts | ✅ |
