# ADR-0005: Linear Rec.2020 working space with opt-in HLG export

Date: 2026-09-22
Status: Superseded by ADR-0011

## Context

Sources mix SDR JPEGs, Apple gain-map HDR HEICs and HLG iPhone video. Output
must look right on SDR screens by default and support HDR when asked.

## Decision

- The compositor works in linear light with Rec.2020 primaries in
  `rgba16float` textures. Every source is converted on decode.
- SDR export applies a fixed BT.2390-style tone map to BT.709 8-bit, tuned so
  pure-SDR inputs pass through unchanged (tested bit-exact within ±1/255).
- HDR export is opt-in, produces HEVC Main10 HLG BT.2020, and is only offered
  when the timeline contains HDR sources. SDR sources inside an HDR export
  are not brightened.
- Rejected: PQ/HDR10 output as default (worse compatibility with iPhone and
  YouTube playback than HLG), per-clip tone-mapping controls in v1.

## Consequences

- One shader path for preview and export; correctness is verified with
  numeric fixtures.
- Apple gain-map decoding requires ImageIO on macOS; on Windows such photos
  fall back to their SDR base image.
