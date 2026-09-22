# ADR-0006: Command pattern with inverse commands for undo

Date: 2026-09-22
Status: Accepted

## Context

Bulk edits on hundreds of clips must be undoable as a single step; the model
must stay testable and serialisable.

## Decision

Every mutation of a `Project` is a `Command` value. `apply(&mut Project,
Command) -> AppliedCommand` returns the inverse. `History` keeps undo and
redo stacks of applied commands. Bulk operations are single commands that
carry the whole selection. Property tests apply random command sequences and
assert that undoing them all restores the exact original project.

## Consequences

- Undo/redo is uniform and cheap; commands can be logged for crash recovery
  and replayed in tests.
- Every new feature that changes the project must be expressed as a command
  first; the UI can never mutate the project directly.
