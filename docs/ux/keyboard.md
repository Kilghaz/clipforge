# Keyboard map (planned, filled in as milestones land)

Rationale for the grid and timeline keys:
`docs/ux/decisions/keyboard-navigation.md`.

| Key | Action | Since |
|---|---|---|
| Cmd/Ctrl+, | Settings | M0 |
| Esc | Close the Settings dialog | M3 |
| Esc, Cmd/Ctrl+W (export window) | Close the export window; a running export keeps going | M6 |
| Cmd/Ctrl+Q | Quit | M0 |
| Space | Play / pause | M3 |
| J / K / L | Reverse / pause / forward | M3 |
| ← / → | Previous / next frame | M3 |
| Cmd/Ctrl+A | Select all clips (timeline) / all items (library, when it has focus) | M2 |
| Esc | Clear selection (library or timeline); cancel a library drag or a trim in progress | M3 |
| End | Playhead to the end | M3 |
| ← → ↑ ↓ | In a choice group: move the selection | M3 |
| ← → ↑ ↓, Home / End, Page Up / Down | Library grid: move focus; selection follows | M3 |
| Shift + arrows | Library grid: extend the selection from the anchor | M3 |
| Ctrl/Cmd + arrows | Library grid: move focus only | M3 |
| Space | Library grid: toggle the focused item (Space still plays in the editor) | M3 |
| Enter | Library grid: add the selection (or the focused item) to the timeline | M3 |
| ↑ / ↓ | Timeline: previous / next clip, playhead to its start (works anywhere in the editor) | M3 |
| Shift + ↑ / ↓ | Timeline: extend the clip selection | M3 |
| Ctrl/Cmd + ↑ / ↓ | Timeline: move focus only | M3 |
| Enter | Timeline (strip focused): toggle the focused clip | M3 |
| Alt/Option + ← / → | Timeline: move the selected clips one place earlier / later (one undo step) | M3 |
| Delete | Remove selected clips | M2 |
| Cmd/Ctrl+Z, Shift+Cmd/Ctrl+Z | Undo / redo | M2 |
| Cmd/Ctrl+D | Duplicate selection (planned, not wired yet) | M4 |
| T | Focus the transition picker in the inspector (then ↑ ↓ to choose, Return to apply) | M4 |
| Tab to the music lane, Enter / Space | Select the music (the inspector shows the playlist and music settings) | M5 |
| Delete (music selected) | Remove all songs (one undo step) | M5 |
| ← → ↑ ↓, Home / End | Title background swatches: move the selection | M5 |
| Cmd/Ctrl+T | Add a text at the playhead (selected, typing replaces the placeholder) | M5 |
| Mouse wheel / trackpad over the timeline | Scroll the timeline sideways (dragging never scrolls; it moves, trims or scrubs) | M5 |
| Tab to the text lane, ← / → | Move the focus between texts in time order (the playhead follows) | M5 |
| Enter / Space (text lane) | Select the focused text; Enter again edits it in place | M5 |
| Space / Enter on "Advanced" (export window) | Show or hide the advanced export settings | M6 |
| Font field: type, ↑ / ↓, Enter, Esc | Filter the fonts, move through the list, pick, close and restore | M5 |
| ← / → (alignment group) | Left / centre / right alignment | M5 |
| ← → ↑ ↓ (texts selected) | Nudge the selected texts by 0.5 % of the frame; Shift: 5 % | M5 |
| Enter (one text selected) | Edit the text in place on the preview | M5 |
| Esc (editing / dragging a text) | End editing / put the dragged text back | M5 |
| Delete (texts selected) | Remove the selected texts (one undo step) | M5 |
| (typing in a text field) | Letters, Space, Delete and arrows edit the text; editor shortcuts wait until the field loses focus | M5 |
