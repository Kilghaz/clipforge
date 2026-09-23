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
- [ ] The packaged `.dmg` from CI artefacts mounts, the app starts after the
      one-time Gatekeeper "Open Anyway" (see README), and
      `codesign --verify --deep --strict ClipForge.app` reports "valid on disk".
- [ ] The NSIS installer from CI artefacts installs and starts on Windows
      after the SmartScreen "Run anyway".

## Milestone 1

- [ ] Dragging files or folders from Finder/Explorer onto the window shows the
      blue drop overlay; releasing imports them (one import for the whole drop).
- [ ] Add folder… with a few hundred photos: rows appear immediately, the
      thumbnails fill in within seconds, scrolling stays smooth.
- [ ] Search, type filter and sort react instantly; the item count is right.
- [ ] Clicking a cell shows details and a larger preview in the inspector;
      HDR videos show the HDR badge.
- [ ] An iCloud Drive file that is not downloaded shows the cloud badge and
      "Not downloaded"; no download is triggered by scrolling or selecting.
- [ ] Remove from library removes the item, the file stays on disk.
- [ ] Show in folder opens Finder/Explorer with the file selected.
- [ ] Restarting the app keeps the library and shows cached thumbnails at once.
- [ ] `CLIPFORGE_IMPORT=/path/to/folder cargo run -p clipforge-app` imports at
      startup (developer shortcut).
