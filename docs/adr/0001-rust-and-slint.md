# ADR-0001: Rust with a Slint UI over a UI-free core

Date: 2026-09-22
Status: Accepted

## Context

The app must be fast, stable with thousands of media items, native on macOS
and Windows, and must not use web technology. The UI should be replaceable
without rewriting the logic, and the logic must be unit-testable without a
window.

## Decision

- The whole workspace is Rust (stable, pinned in `rust-toolchain.toml`).
- The UI is Slint (winit backend, femtovg renderer, `cupertino` style on
  macOS and `fluent` on Windows, chosen at build time). Slint's wgpu
  integration (`renderer-femtovg-wgpu` / `unstable-wgpu-30`) is the planned
  path for showing the preview texture; it is verified by a spike before
  Milestone 2 starts.
- Only `crates/app` depends on Slint. All state lives in Rust view models;
  `.slint` files contain layout and bindings only.
- Rejected: Tauri/Electron/Dioxus (web), SwiftUI + WinUI (two UIs), egui
  (weaker text/accessibility; kept as fallback because the `app` crate is
  thin), GPUI (macOS-centric maturity at decision time).

## Consequences

- Cross-platform with one codebase and native-looking widgets.
- Slint's Royalty-free Licence must be honoured (see `LICENSES.md`).
- Generated Slint code needs lint allowances; they are confined to the `ui`
  module in `crates/app/src/main.rs`.
- If Slint blocks us (timeline performance, texture import), swapping to egui
  touches only `crates/app`.
