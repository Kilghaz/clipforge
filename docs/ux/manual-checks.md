# Manual checks

Things CI cannot verify. Run before tagging a milestone.

## Milestone 0

- [ ] `cargo run -p clipforge-app` opens a window with menu bar
      (File, Edit, View), the library panel on the left and the editor on
      the right. The divider resizes the panel (double-click resets it); the
      panel button / View menu hides it; the gear button opens Settings as
      a dialog (Escape closes it).
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
      follows the pointer, turns blue over the strip, the accent-coloured marker shows
      the insertion point, and releasing inserts the photos there.
- [ ] Dragging the divider resizes the library panel smoothly without the
      panel jittering or thumbnails flickering.
- [ ] Click, shift-click and Cmd/Ctrl-click select clips as expected;
      Cmd/Ctrl+A selects all; Delete removes; Cmd/Ctrl+Z / Shift+Cmd/Ctrl+Z undo/redo.
- [ ] Drag a clip (or a selection) to a new position; the accent-coloured marker
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
- [ ] Drag the right edge of a video clip: the edge stays exactly under the
      pointer (later clips move along) until the end of the video; the left
      edge likewise, with the clip's right edge staying put and the gap
      closing on release. The preview shows the new first/last frame, the
      duration updates live, release trims in one undo step, Escape during
      the drag restores the original length.
- [ ] Inspector shows Mute and Volume for video selections; muting removes the
      ▶ badge's sound icon and the exported file has no audio for that clip.
- [ ] A dissolve between two videos plays both pictures blended; the export
      cross-fades the sound as well.
- [ ] Export with videos: the MP4 has an AAC track in sync with the picture
      (check a clap or a beat against the frame in a player).
- [ ] 4K HEVC iPhone clip: scrubbing responds within ~0.2 s and playback is
      smooth on the Mac; note the frame rate on Windows.

## Design guide pass (ADR-0009)

- [ ] The whole UI is dark on both platforms; no light widgets appear.
- [ ] Every icon-only button shows a tooltip after a short hover; no emoji
      glyphs remain anywhere.
- [ ] Empty library and empty timeline show the empty-state text with a
      working primary action.
- [ ] Settings dialog and export window: Escape closes them (a running export keeps going);
      button order is `[Primary] [Cancel]` on Windows and reversed on macOS.
- [ ] Library cells and timeline clips: hover tint, accent selection ring;
      video trim handles brighten on hover.
- [ ] Drag ghost shows an icon and "{n} items" and turns accent-coloured over
      the timeline; the timeline shows a tinted drop zone.
- [ ] German: all new strings translated, nothing truncated in the inspector
      or the library action bar at the minimum window width.
- [ ] Icon-only buttons: icon centred, hover and pressed tints visible,
      keyboard focus ring visible when tabbing; duration presets readable when
      selected.

## Component audit pass (2026-09-24)

- [ ] Duration presets, framing, orientation, export resolution and quality
      are choice groups: one Tab stop each, Left/Right (or Up/Down) move the
      selection, the selected option shows the focus ring.
- [ ] Library: Ctrl/Cmd+A selects everything, Esc clears the selection, Esc
      during a drag abandons it; the × in the action bar clears too.
- [ ] Timeline: Esc clears the clip selection; End jumps to the end.
- [ ] Dialogs: Enter triggers the primary action (Export…, Close); the
      Settings entry is disabled while the export dialog is open.
- [ ] Search: clear button hidden when disabled; the field dims when disabled.
- [ ] Library empty state does not appear while a scan/import is running;
      "no results" offers "Clear search and filter" and names the type filter
      when the search box is empty.

## Milestone 4

- [ ] Transition picker lists 13 kinds; each plays as named in the preview
      (scrub through an overlap): dissolve, fades through black and white,
      slides and wipes in all four directions, zoom from the centre.
- [ ] With nothing selected, choosing a transition or motion changes every
      clip in one undo step and becomes the default for clips added later.
- [ ] "Shuffle transitions" / "Shuffle motion" give neighbouring clips
      different kinds, report "Shuffled … on N clips/photos" in the transport
      bar for ~5 s, and undo in one step. Each click gives a new mix.
- [ ] Motion section appears only when photos are among the targets; with
      a mixed selection it says "Applies to photos only". Videos never move.
- [ ] Ken Burns: zoom in/out and pans are smooth, start and stop softly and
      never show a black edge; the export matches the preview.
- [ ] The first clip plays its transition in from black (white for the white
      fade); a slide pushes the first photo in over black.
- [ ] A transition longer than half of a neighbouring clip shows "Shortened
      where clips are too short" under the duration slider.
- [ ] `T` jumps to the transition picker; ↑ ↓ change it; Return applies.
- [ ] Timeline: moving photos show the move badge; every transition shows a
      tinted overlap with a thin edge that is visible on bright and dark
      pictures; badges are not hidden under the next clip.

## Keyboard access to grid and timeline (2026-09-25)

- [ ] Tab from the sort button lands in the library grid; a white focus ring
      appears on the entry item (last focused, else first selected, else first).
      Clicking a cell hides the ring; the next arrow key shows it again from
      the clicked cell.
- [ ] Arrows move focus and selection; Shift extends; Ctrl/Cmd moves the ring
      only; Space toggles; Page Up/Down jump by visible rows; the focused row
      scrolls into view at both ends of a long library.
- [ ] Enter in the grid adds the selection to the timeline.
- [ ] ↑/↓ anywhere in the editor select the previous/next clip, move the
      playhead to its start, and scroll the strip to keep the clip visible.
      ←/→ still step frames after clicking a clip.
- [ ] Alt/Option+←/→ moves the selected clips by one place; Cmd/Ctrl+Z undoes
      each press separately.
- [ ] Tab reaches the timeline strip; Enter toggles the focused clip.
- [ ] VoiceOver / Narrator announce the grid and strip as lists, items with
      their selected state, clips with duration and "muted".

## Milestone 5

- [ ] Library → select an audio file and "Add to timeline" (or double-click,
      or drag it onto the timeline): the song appears on the music lane, not
      as a clip; photos in the same selection become clips; one undo removes
      both.
- [ ] "Add music…" (lane, Music inspector or Edit menu) picks audio files;
      they are imported and appear as songs a moment later ("Added N songs").
- [ ] The lane shows songs back to back on the clips' time scale; with Loop
      on, the playlist repeats (dimmed) until the show ends; the fade-out
      darkens the end.
- [ ] Clicking the lane selects the music (ring) and the inspector shows
      Songs, Volume, Fades, Length; Escape, a clip click or empty strip space
      leaves it. Tab reaches the lane; Enter selects it.
- [ ] Move up / down / remove reorder the playlist; each is one undo step;
      Delete with the music selected removes all songs.
- [ ] Playback: music plays from the playhead, fades in and out, and gets
      quieter under a video clip with sound (Lower under video sound);
      the export has the same mix.
- [ ] "Fit slideshow to music" makes the show as long as the songs (videos
      and title cards keep their length); the read-out shows the two lengths.
- [ ] Text track: the toolbar T button, Cmd/Ctrl+T, Edit → Add text and a
      double-click on the empty text lane add "Your text" at the playhead,
      selected, with the text field open on the preview; typing replaces it.
- [ ] On the preview: drag a text to move it (snaps to the centre lines with
      a guide), side handles change its width, corner handles its size;
      each drag is one undo step and Escape during a drag puts it back.
      Double-click (or Enter) edits in place with the same font; Escape or a
      click outside ends editing; one Cmd/Ctrl+Z undoes the typing session.
- [ ] Arrows nudge the selected text; Delete removes it; clicking empty
      preview space deselects.
- [ ] On the text lane: drag a text block to move it in time, its edges to
      trim it; overlapping texts stack into rows.
- [ ] Inspector: text; font field lists installed and bundled fonts, each in
      its own font, filters while typing (↑/↓, Enter, Esc); size in pt
      (number field); bold, italic, underline; alignment icons; colour,
      shadow, box behind text and its colour, duration, animation in and out
      (fade, slides, wipes, zoom) — all update the preview at once and apply
      to all selected texts.
- [ ] A project using a font that is not installed opens with the default
      font instead (no crash, no missing text).
- [ ] Texts animate in the export exactly as in the preview; texts stay on
      top of clip transitions.
- [ ] Opening a project saved with captions shows them as texts.
- [ ] "Add opening title" inserts a black colour card and a large text over
      it with the text field open; "Add closing card" does it at the end.
      Colour cards take a background colour and hide framing and motion.
- [ ] German: every new string translated; song counts say "Musikstücke";
      nothing is cut off in the inspector at 900 px.

## Milestone 6 — HDR and export

- [ ] With an iPhone HLG video on the timeline the preview looks natural
      (not flat, not clipped); highlights roll off softly.
- [ ] Export dialog: the HDR switch is off and disabled without HDR videos
      (caption says why); with one it can be turned on; the summary line
      switches to HEVC and the size changes with quality and resolution.
- [ ] HDR export plays in HDR on an iPhone / Mac (QuickTime shows "HDR"),
      photos and texts look like in the SDR export, not glaring.
- [ ] SDR export of the same timeline: HLG video looks like the preview.
- [ ] Advanced: codec, frame rate, bitrates and file format change the
      summary; HDR locks the codec, YouTube locks MP4; a MOV export opens in
      QuickTime.
- [ ] Export opens its own window (title "Export video"); the editor stays
      usable next to it; Export… again brings the same window to the front.
- [ ] During an export: closing the window (Close, Esc, Cmd/Ctrl+W, title
      bar) keeps the export going; the toolbar shows the percentage; editing
      and playback keep working; clicking the toolbar button brings the
      window back with progress and time left.
- [ ] Quitting via the main window also closes the export window.
- [ ] After the export: toolbar shows "Exported"; the dialog says where the
      file is and "Show in Finder / Explorer" selects it.
- [ ] Cancel export removes the partial file.
- [ ] Making the export window small scrolls its content; the buttons stay
      visible; toggling Advanced resizes the window to fit.

## Milestone 7 — Windows parity

- [ ] OneDrive "Files On-Demand" folder (online-only files): import shows
      the cloud badge and reads nothing; "Download {n}" in the selection bar
      downloads them with progress and Cancel; afterwards thumbnails appear
      and the files can go on the timeline.
- [ ] Same with iCloud for Windows and macOS iCloud Drive (optimised
      storage); on macOS the `.icloud` stubs download too.
- [ ] Adding a cloud-only file to the timeline says it is not downloaded yet.
- [ ] "Free up space" on a timeline file, then Export: the window says the
      file will be downloaded; the export works.
- [ ] Windows without "HEVC Video Extensions": export with HDR or HEVC shows
      the Store hint and the button opens the Store page; with the extension
      installed the hint is gone.
- [ ] Windows title bars (main and export window) are dark even with the
      light Windows theme; same on macOS in light mode.
- [ ] Export on a PC with NVIDIA / Intel / AMD graphics uses the hardware
      encoder (the export report names it); on a PC without, it falls back
      to software without an error.
