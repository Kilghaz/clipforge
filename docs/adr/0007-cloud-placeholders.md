# ADR-0007: Files stay in place; cloud placeholders are first-class

Date: 2026-09-22
Status: Accepted

## Context

Cloud photo APIs are closed or restricted (PhotoKit is macOS-only, Google
Photos Library API was withdrawn for third parties in 2025, iCloud has no
Windows API). Users still have iCloud Drive, iCloud for Windows and OneDrive
folders with files that are not downloaded.

## Decision

- ClipForge does not integrate cloud APIs. It links to files wherever they
  are and never copies them, with one exception: drops from Apple Photos
  arrive as temporary exports and are copied into a managed folder, visibly.
- Cloud placeholders are detected via OS attributes
  (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`/`OFFLINE` on Windows, ubiquitous
  item status and `.icloud` stubs on macOS). They display the OS-provided
  thumbnail and a cloud badge.
- Downloading (hydration) happens only as an explicit, visible job: when the
  user asks for a full-size preview or when export needs the file.

## Consequences

- Works identically for every provider that uses the OS placeholder
  mechanism; unknown providers are treated as local files.
- Export can fail on offline files; the planner checks hydration first and
  asks before downloading gigabytes.
