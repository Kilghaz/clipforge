# ADR-0003: Decode in-process with libav, encode through an ffmpeg sidecar

Date: 2026-09-22
Status: Accepted

## Context

Export is the longest-running and most crash-prone operation. We need
hardware encoders on both platforms (VideoToolbox, Media Foundation, NVENC,
QSV, AMF), correct HDR signalling and clean licensing.

## Decision

- Decoding uses `ffmpeg-next` (libavcodec/libavformat) in-process, with
  hardware acceleration (VideoToolbox on macOS, D3D11VA on Windows), plus
  Apple ImageIO for HEIC gain-map HDR photos.
- Encoding spawns the bundled `ffmpeg` executable, feeds rendered frames and
  mixed PCM audio over pipes, and parses `-progress pipe:` output. The
  planner (`clipforge-export::EncodePlan`) is a pure function and picks the
  encoder after probing what the binary offers.
- Rejected: in-process libavcodec encoding (a crash kills the app, GPL
  linking questions), native Media Foundation code (duplicate work), and a
  sidecar for decoding too (latency for scrubbing; revisit if decoder
  crashes become a problem, see Milestone 8 watchdog).

## Consequences

- Export crashes are recoverable and reportable; the app keeps working.
- The GPL ffmpeg binary stays a separate program (see `LICENSES.md`).
- Pipe throughput must sustain 4K 10-bit frames; the frame pump is
  benchmarked in Milestone 6.
