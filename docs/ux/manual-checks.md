# Manual checks

Things CI cannot verify. Run before tagging a milestone.

## Milestone 0

- [ ] `cargo run -p clipforge-app` opens a window with menu bar
      (File, View) and sidebar (Library, Editor, Settings).
- [ ] Settings → Interface language → "Deutsch" switches all visible strings
      immediately, including the menu and the "System language" entry.
- [ ] Restarting the app keeps the chosen language.
- [ ] "System language" follows the OS language (German macOS/Windows shows
      German).
- [ ] macOS uses the Cupertino style, Windows the Fluent style.
- [ ] File → Quit closes the app cleanly (no log errors).
- [ ] The packaged `.app`/installer from CI artefacts starts.
