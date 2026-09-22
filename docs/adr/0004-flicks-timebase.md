# ADR-0004: Integer time in flicks

Date: 2026-09-22
Status: Accepted

## Context

Slideshow timing mixes 24/25/30/60 fps video, 44.1/48 kHz audio and
user-entered durations. Float seconds drift and make equality tests flaky.

## Decision

All durations and positions in the model are `core::Ticks`, an `i64` count
of flicks (1/705 600 000 s). One flick divides evenly into every supported
frame and sample rate, so frame ↔ time conversions are exact. Floats appear
only at the boundary to display and to formats that store floats.

## Consequences

- Exact arithmetic, deterministic tests, no accumulated error.
- `i64` flicks cover ±414 years; overflow is not a practical concern but
  `checked_*` variants exist.
- Frame rates are rational `num/den`; only rates that divide a flick-second
  are constructible.
