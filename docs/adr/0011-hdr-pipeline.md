# ADR-0011: HDR as a separate 16-bit HLG path; tone mapping on decode

Date: 2026-09-25
Status: Accepted (supersedes ADR-0005)

## Context

ADR-0005 planned one linear Rec.2020 `rgba16float` working space for every
source. Since then the compositor became a quad renderer (ADR-0010) that
works on 8-bit sRGB textures, the SDR path is tuned and tested against the
CPU compositor, and nearly all timelines are SDR only. The bundled ffmpeg
has no `zscale`, so HDR conversion in the sidecar is not available. Apple
gain-map photos need ImageIO bindings that are not in the tree and no CI
fixture exists for them.

## Decision

- **SDR output stays 8-bit sRGB / BT.709** end to end. SDR sources are not
  converted at all, so they pass through bit-exactly.
- **HDR video (HLG, PQ) decodes to 16-bit BT.2020 R'G'B'** in its own
  transfer (`VideoFrame::Hdr`, ffmpeg `rgb48le` with a BT.2020 matrix).
- **For SDR output** such frames are tone mapped in Rust
  (`render::colour`): HLG inverse OETF + OOTF (γ 1.2 at 1000 cd/m²) or PQ
  EOTF, a BT.2390-style luminance roll-off from 80 % of SDR white, BT.2020 →
  BT.709 with desaturation of out-of-gamut colours, sRGB encode. Tables
  replace the transfer functions; `export::sdr_frame` converts in bands on
  all cores (4K ≈ 25 ms).
- **HDR export is opt-in** (only with HLG/PQ video on the timeline and a
  GPU): the GPU compositor renders into `Rgba16Float` targets with a second
  pipeline. HLG video textures keep their signal; SDR sources, texts and
  colour cards are converted in the shader to HLG with SDR white at the
  BT.2408 reference level (HLG 0.75, 203 cd/m²), so they keep their look.
  PQ sources are converted to HLG on the CPU. Frames are read back as
  16-bit, converted to 10-bit BT.2020 NCL limited-range 4:2:0 and encoded
  as HEVC Main10 HLG (`arib-std-b67`).
- The CPU compositor has no HDR path; without a GPU the HDR option is
  disabled with an explanation.
- **Deferred:** Apple gain-map photos (shown as their SDR base image), PQ /
  HDR10 output, HDR preview on HDR displays.
- Rejected: converting every source to linear float (costs the SDR path
  bandwidth and bit-exactness for a feature few timelines use).

## Consequences

- The SDR path is unchanged and as fast as before; HDR costs only when used.
- Two render paths for texture colour (sRGB vs HLG) must stay in step; the
  shader and `colour::sdr_to_hlg` share the maths and a GPU test compares
  them.
- Gain-map HDR photos need a later ADR when ImageIO bindings land.
