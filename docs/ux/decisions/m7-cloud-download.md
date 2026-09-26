# M7 cloud download — behaviour decisions

The user wants to use photos and videos that live in OneDrive, iCloud for
Windows or iCloud Drive but are not on this computer yet, without surprise
downloads (ADR-0007).

References: DESIGN.md §1 "Long-running work" (NN/G visibility of system
status, NN/G progress indicators: percent done, cancel), Primer
"Notification messaging" (say what happened), Fluent command bar
(contextual actions for the selection).

## Decisions

- **Explicit only.** Nothing is downloaded by scrolling, selecting or
  previewing. The grid shows the cloud badge ("Not downloaded").
- **Where:** whenever the selection contains cloud files, the selection bar
  shows "Download {n}" (cloud icon) in place of "Add to timeline" and "Add
  all": downloading has to come first, and the bar must fit a 280 px panel
  (German labels overflowed with both, scene `narrow-download`). Enter and
  dragging still add the local part of a mixed selection and name the
  skipped cloud files. The details panel of a single cloud file has a
  "Download" button next to "Show in folder".
- **Feedback:** the library status bar shows "Downloading 2 of 5 · 340 MB of
  1.2 GB…" (German "Herunterladen: 2 von 5 · …") with a determinate bar and a cancel button (24 px, tooltip
  "Cancel download"); afterwards "Downloaded 5 files." or "Downloaded 4
  files; 1 could not be downloaded." or "Download cancelled."
- **After download** each file is fingerprinted, probed and thumbnailed like
  a fresh import; it can then go on the timeline.
- **Adding cloud files** to the timeline skips them with the status message
  "{n} files are not downloaded yet. Select them and choose Download."
- **Export** of a project whose files went back to cloud-only (OneDrive
  "Free up space"): the export window says "{n} files are only in the cloud
  and will be downloaded during the export." Reading them downloads them.
- **Undo:** none; downloading does not change the project.
- **Keyboard:** the buttons are in the Tab order; no shortcut.
- **Platform:** Windows Cloud Files and macOS File Provider hydrate on read;
  macOS `.icloud` stubs go through `brctl download`.
- **Scenes:** `library-download`, `narrow-download` (selection bar with
  Download, status bar progress).

## Tests first

- `platform::cloud::tests::hydrate_*` (read-through, cancel, stubs)
- `library::tests::download_turns_placeholders_into_probed_local_items`
- `library::tests::a_download_that_cannot_reach_the_file_counts_as_failed`
- `export_view::tests::cloud_files_on_the_timeline_are_counted`
