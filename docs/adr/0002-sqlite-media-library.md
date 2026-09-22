# ADR-0002: SQLite catalogue with a file-based thumbnail cache

Date: 2026-09-22
Status: Accepted

## Context

The library must handle tens of thousands of linked files, search and sort
instantly, survive moved files, and never copy the user's media. A
"proper multimedia database" was requested for speed.

## Decision

- Metadata lives in one SQLite database (`rusqlite`, bundled, WAL mode,
  `synchronous=NORMAL`, FTS5 for text search). This is the same choice as
  Lightroom, Apple Photos and digiKam.
- Thumbnails and proxies are files in a content-addressed cache directory
  keyed by `Fingerprint` (size + xxh3 of head and tail), not BLOBs.
- Files are referenced by path plus fingerprint; relinking after a move is
  a fingerprint lookup.
- Rejected: dedicated media/vector databases (operational weight, no benefit
  at this scale), embedded KV stores like redb/sled (no ad-hoc queries, no
  FTS), BLOB storage of thumbnails (WAL bloat, write amplification).

## Consequences

- Zero administration, single file, trivially testable in memory.
- Speed comes from schema, indexes, the OS thumbnail fast path and the
  cache, so the import pipeline design (docs/PLAN.md §3.4) is the real
  performance work.
- Schema migrations are versioned and tested from every previous version.
