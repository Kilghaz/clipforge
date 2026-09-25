# ADR-0010: GPU compositor via wgpu, CPU compositor as fallback

Date: 2026-09-25
Status: Accepted

## Context

Milestones 2 and 4 shipped a CPU compositor instead of the planned wgpu one
(PLAN.md deviations). It was fast enough in release builds but, with Ken
Burns motion and two-clip transitions, it uses most of a frame budget at 4K
export (73 ms per frame in release) and leaves little headroom for titles,
colour work and slower Windows machines. Issue #1 tracked the switch.

Raw Vulkan was considered: it is not native on macOS (only through
MoltenVK, a translation layer we would have to bundle), and it would mean
writing the same code again for Metal and DX12 to get native drivers.

## Decision

`clipforge-render` gets a `GpuCompositor` on wgpu (Metal on macOS, DX12 or
Vulkan on Windows) with one WGSL shader. It draws a clip as a textured quad
(fit, rotation and the Ken Burns camera are the quad's rectangle) into
offscreen targets and composes transitions as a second pass (alpha,
offsets, scissors and a solid-colour quad). Stills are uploaded once with a
mip chain and cached with an LRU budget; video frames are re-uploaded only
when the decoder hands out a new frame.

Both compositors implement the `FrameRenderer` trait. `best_renderer()`
picks the GPU and falls back to the CPU compositor when no adapter exists
or `CLIPFORGE_RENDERER=cpu` is set. The CPU compositor stays the reference:
`tests/gpu_matches_cpu.rs` compares both over fit modes, rotation, Ken
Burns, all transitions and video frames within a small tolerance, and
skips on machines without a GPU.

The finished frame is read back to RGBA, so the preview (Slint image) and
export (ffmpeg pipe) interfaces are unchanged.

## Consequences

- Frame times on an M3 Max: preview Ken Burns 2.4 → 0.9 ms, preview
  dissolve 5.0 → 1.1 ms, 4K export dissolve 109 → 4 ms (dev profile).
- Two implementations of the same pictures must be kept in step; the
  comparison test is the guard, and new effects need both (or a GPU-only
  effect needs a decision on the CPU fallback).
- CI runners without a GPU exercise only the CPU path.
- The read-back costs a copy per frame; importing the texture into Slint
  (`renderer-femtovg-wgpu`) and a GPU YUV conversion for export are
  possible later steps.
- Supersedes the renderer deviations for M2 and M4 in PLAN.md.
