# ADR-0008: Decode video and audio through streaming ffmpeg sidecars

Date: 2026-09-23
Status: Accepted (supersedes the in-process decoding part of ADR-0003)

## Context

ADR-0003 planned in-process libav decoding for playback. Milestones 1 and 2
showed that driving the `ffprobe`/`ffmpeg` executables is fast enough,
keeps the Windows build free of native FFmpeg libraries, keeps the GPL
binary a separate program, and isolates decoder crashes from the app.
Linking libav would add vcpkg/prebuilt-library handling on Windows CI,
dynamic library bundling on both platforms, and a large unsafe surface.

## Decision

- Video frames for preview, playback and export come from an `ffmpeg`
  child process that seeks to a start time and streams raw RGBA frames at
  the requested size over stdout (`VideoReader`). Seeking restarts the
  process. Hardware decoding is requested via `-hwaccel` (VideoToolbox on
  macOS, D3D11VA on Windows) with automatic software fallback.
- Audio comes the same way as interleaved 48 kHz stereo `f32` PCM
  (`AudioReader`).
- Single frames for scrubbing use one-shot extraction (`frame_at`).
- Every reader kills its child on drop; reads are bounded by frame size so
  a stalled process cannot wedge the UI thread (readers run in jobs or on
  the playback thread).
- In-process libav remains an option if pipe throughput or seek latency
  become the bottleneck; the `SourceProvider` interface hides the choice.

## Consequences

- Seek latency is process spawn + keyframe seek (tens of milliseconds);
  scrubbing is debounced and playback pre-rolls.
- 4K export moves up to ~1 GB/s of RGBA through a pipe; measured
  acceptable on Apple Silicon, and frames are requested at output size.
- No FFI in `media` yet; `unsafe` stays out of the workspace.
