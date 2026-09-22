# AGENTS.md – how to work in this repository

ClipForge is a native macOS/Windows desktop app (Rust + Slint) that turns
thousands of photos and videos into a slideshow video. Read `docs/PLAN.md`
for the product and architecture; this file is the operating manual.

## Architecture in one paragraph

A Cargo workspace. `core` holds the project model, commands and undo and has
no IO. `media` defines decoder traits and backends. `library` is the SQLite
media catalogue plus thumbnail/proxy cache. `render` is the wgpu compositor
(pure function of project and time). `export` plans encodes and drives an
ffmpeg sidecar process. `jobs` is the vocabulary for cancellable background
work. `platform` wraps OS specifics. `i18n` knows the shipped languages.
`app` is the only crate that touches Slint. Dependency direction (enforced by
`cargo xtask check-deps`, which also runs as a unit test):

```
app → {core, library, render, export, jobs, platform, i18n, media}
export → {core, render, media, jobs}      render → {core, media}
library → {core, media, jobs, platform}   media → {core}
platform, jobs, i18n, core → nothing internal
```

## Commands

| Task | Command |
|---|---|
| Everything CI runs | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo nextest run --workspace && cargo test --workspace --doc && cargo xtask check-deps && cargo xtask fixtures --verify && cargo deny check` |
| Tests only | `cargo nextest run --workspace` (or `cargo test --workspace`) |
| One crate | `cargo nextest run -p clipforge-core` |
| Run the app | `cargo run -p clipforge-app` (`RUST_LOG=info` for logs) |
| Regenerate fixtures | `cargo xtask fixtures` (needs ffmpeg with libx264/libx265) |
| Regenerate icons | `cargo xtask icons` (needs `rsvg-convert` or macOS) |
| Licence report | `cargo about generate about.hbs -o target/THIRD_PARTY_LICENSES.html` |
| Package | `cargo build --release -p clipforge-app && cargo packager --release -p clipforge-app` → `dist/` |

Tooling: Rust pinned in `rust-toolchain.toml`; `cargo install cargo-nextest cargo-deny cargo-insta cargo-packager` and `cargo install cargo-about --features cli`.

## Rules

1. **Tests first.** Write the failing test, then the minimal code, then
   refactor. Unit tests live next to the code (`#[cfg(test)] mod tests`);
   integration tests in `crates/<name>/tests/`. Use `proptest` for
   invariants, `insta` for snapshots, golden images with tolerance for
   rendering. Never depend on personal media: use `fixtures/`.
2. **Commands are the only way to change a project.** Every user action in
   the editor is a `Command` in `core` with an inverse. Bulk edits are one
   command, one undo step.
3. **No IO, threads, OS or UI code in `core` and `render`.** If you need it,
   you are in the wrong crate. `check-deps` will fail otherwise.
4. **All slow work is a job**: cancellable via `CancellationToken`, reports
   `Progress`, runs off the UI thread, has a `Priority`.
5. **No `unwrap()`, `expect()`, `panic!`, `todo!` outside tests.** Clippy
   denies them. Library errors use `thiserror`; only `app` may use `anyhow`.
6. **`unsafe` only in `media` and `platform`** (FFI), each block with a
   `// SAFETY:` comment. Every other crate has `#![forbid(unsafe_code)]`.
7. **Every user-facing string goes through `@tr()`** in `.slint` files and
   gets a German entry in `crates/app/lang/de/LC_MESSAGES/clipforge-app.po`.
   Strings Rust needs come from the Slint `Strings` global, never from Rust
   literals.
8. **No web technology.** No Tauri, Electron, WebView, HTML/CSS. `cargo deny`
   bans the usual crates.
9. **Time is integer flicks** (`core::Ticks`). No `f64` seconds in the model.
10. **New dependency?** Justify it in the PR/commit message and make sure
    `cargo deny check` passes. Prefer crates already in the tree.
11. **Keep the UI thin.** `.slint` files and Slint callbacks contain no
    domain logic; they call into view models in `app` that are unit-tested
    without a window.
12. **Architecture decisions get an ADR** in `docs/adr/` (copy
    `0000-template.md`). Changing a decision means a new ADR that supersedes
    the old one, not an edit.

## Definition of done

- Tests written and green on macOS and Windows CI.
- `cargo fmt`, `cargo clippy -D warnings`, `cargo deny check` clean.
- New strings translated; new commands/jobs documented in
  `docs/ARCHITECTURE.md` if they add a concept.
- No performance regression against the budgets in `docs/PLAN.md` §3.4.
- Commit messages: conventional (`feat(core): …`, `fix(library): …`,
  `test(render): …`, `docs: …`, `chore: …`).

## Recipes

**Add a command (core):** add a variant to `Command`, implement `apply` that
returns the inverse, add a proptest case to the "undo restores original"
suite, expose it in the `app` view model, wire a Slint callback.

**Add a transition (render):** add a WGSL shader in `render/shaders/`, a
`TransitionKind` variant, a golden test at progress 0 / 0.5 / 1, a name in
the `.slint` transition palette with `@tr()`, and a German translation.

**Add an export option:** extend `ExportOptions`, map it in
`EncodePlan::build`, extend the planner tests, then the dialog.

**Add a fixture:** extend `xtask/src/fixtures.rs`, run `cargo xtask fixtures`,
keep the total under 200 KB, commit `fixtures/` and `MANIFEST.txt`.

## Layout

```
crates/core      domain model, commands, time      crates/render    wgpu compositor
crates/media     decoder traits + backends         crates/export    planner + ffmpeg sidecar
crates/library   SQLite catalogue, cache           crates/platform  OS glue
crates/jobs      priorities, cancellation          crates/i18n      languages
crates/app       Slint UI (ui/*.slint, lang/*.po)  xtask/           dev tasks
docs/PLAN.md     the plan                          docs/adr/        decisions
docs/ARCHITECTURE.md  living overview              fixtures/        generated test media
```
