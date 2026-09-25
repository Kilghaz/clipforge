# ExportStatusButton (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/components.slint` → `ExportStatusButton` (editor toolbar)
Gallery rows: "Disclosure …; ExportStatusButton running 42 %, done, done with warning, failed"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| NN/G | https://www.nngroup.com/articles/progress-indicators/ | percent-done for long work, approximate time, let users stop |
| NN/G | https://www.nngroup.com/articles/visibility-system-status/ | status stays visible while users do other things |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ProgressBar.html | determinate bar, value text |

## Anatomy / states

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| running | percent + bar | "42 %" and a 2 px accent bar along the bottom | ✅ | gallery, `export-background` |
| done | say what happened | check icon (success) + "Exported" | ✅ | gallery |
| done with warning | | warning icon + "Exported" (details in the dialog) | ✅ | gallery |
| failed | | error icon + "Failed" | ✅ | gallery |
| hover / pressed | | control hover / pressed tints | ✅ | |
| focus | ring | 2 px focus ring | ✅ | |
| time left | NN/G approximate | in the dialog only (toolbar space) | ➖ | `export-running` |
| cancel | NN/G let users stop | in the dialog ("Cancel export"), one click away | ➖ | |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | controls 32 | 32 | ✅ |
| min width | Export button ~ 120 | 96, keeps the toolbar from jumping in narrow windows | ✅ |
| icon | 16/20 | 20 | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| click reopens the dialog | | sets `export-open`, clears "unseen" | ✅ | manual-checks M6 |
| Space / Enter | button | FocusScope | ✅ | |
| shown when | status visible while working | export running, or finished while the dialog was closed | ✅ | |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| name and value | progressbar value | label "Show export", value = the shown text | ✅ |
