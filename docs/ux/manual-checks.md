# Manual checks

Things CI cannot verify. Run before tagging a milestone.

## Milestone 0

- [ ] `cargo run -p clipforge-app` opens a window with menu bar
      (File, Edit, View), the library panel on the left and the editor on
      the right. The divider resizes the panel; ◧ / View menu hides it;
      ⚙ opens Settings as an overlay.
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

## Milestone 2

- [ ] Library panel → "Add all" with ~200 photos: clips appear with
      thumbnails, the preview shows the first photo. Double-click adds one.
- [ ] The ▴ button in the library panel opens the details of the selected
      item (Show in folder, Remove).
- [ ] Library selection: click, shift-click range, Cmd/Ctrl-click toggle,
      and a rubber band dragged from empty space between cells (shift or
      Cmd/Ctrl while starting the band adds to the selection). "Add N to
      timeline" reflects the count.
- [ ] Drag one or more selected library cells onto the timeline: a badge
      follows the pointer, turns blue over the strip, the orange marker shows
      the insertion point, and releasing inserts the photos there.
- [ ] Dragging the divider resizes the library panel smoothly without the
      panel jittering or thumbnails flickering.
- [ ] Click, shift-click and Cmd/Ctrl-click select clips as expected;
      Cmd/Ctrl+A selects all; Delete removes; Cmd/Ctrl+Z / Shift+Cmd/Ctrl+Z undo/redo.
- [ ] Drag a clip (or a selection) to a new position; the orange marker
      shows the drop point.
- [ ] With nothing selected, the duration slider changes every clip in one
      undo step; with a selection, only those clips.
- [ ] Cross dissolve applied to all: clips overlap in the strip; scrubbing
      through an overlap shows the blend; Space plays the show.
- [ ] Fit "Fill frame" crops a portrait photo in a landscape project;
      switching the project to 9:16 re-renders the preview.
- [ ] Save, quit, restart: the autosaved project comes back; Open loads a
      saved `.clipforge.json`.
- [ ] Export… Full HD, Better, YouTube: progress advances, the MP4 plays in
      QuickTime/VLC with the right durations and transitions; Cancel leaves
      no file behind.

## Milestone 3

- [ ] Add a video from the library (double-click / drag): the clip shows a
      ▶ badge, its natural duration and a frame in the preview when scrubbed.
- [ ] Space / L plays with sound; K pauses; J jumps back a second; ←/→ step
      one frame (Shift: ten); Home returns to the start. Audio stops on pause
      and on scrubbing.
- [ ] Drag the left or right edge of a video clip: the white marker follows,
      the preview shows the new first/last frame, release trims (undoable).
- [ ] Inspector shows Mute and Volume for video selections; muting removes the
      ▶ badge's sound icon and the exported file has no audio for that clip.
- [ ] A dissolve between two videos plays both pictures blended; the export
      cross-fades the sound as well.
- [ ] Export with videos: the MP4 has an AAC track in sync with the picture
      (check a clap or a beat against the frame in a player).
- [ ] 4K HEVC iPhone clip: scrubbing responds within ~0.2 s and playback is
      smooth on the Mac; note the frame rate on Windows.
