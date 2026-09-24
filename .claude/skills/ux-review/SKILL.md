---
name: ux-review
description: The post-change UI review for ClipForge - renders the app scenes and the component gallery, walks the spec cards of every touched component line by line, runs the DESIGN.md heuristics and visual checklist, and reports findings with severity. Refuses to pass a touched component that has no spec card or no gallery row. Use after any change under crates/app/ui or to view models - "review the UI", "ux review", "visual pass", "is this done from a UX point of view".
---

# ux-review — did the change meet the guide?

Run after implementing, before calling UI work done (AGENTS.md rule 13 and
the definition of done). Output is a findings list, most severe first; the
user triages, you fix what is chosen, then re-run until clean.

## Steps

1. **Scope.** `git diff --name-only` (or the branch diff). List touched
   `.slint` files and the components inside them, plus view models whose
   behaviour changed.
2. **Preconditions.** For every touched component:
   - a spec card exists in `docs/ux/components/` → else finding *blocker*:
     "no spec card, run ux-component-spec".
   - every state the component has appears in `crates/app/ui/gallery.slint`
     → else finding *blocker*: "state X has no gallery row".
   - strings: `@tr()` everywhere and a German entry in
     `crates/app/lang/de/LC_MESSAGES/clipforge-app.po` (diff `@tr("…")` in
     the touched files against msgids).
3. **Render.** `cargo run -p clipforge-app --bin screenshot` (all scenes
   including `gallery` at 2×). Open every PNG the change can affect and look.
   Crop with `sips -c H W --cropOffset Y X in.png --out out.png` when a detail
   needs a closer look. Do not skip this step; do not reason about pixels you
   have not seen.
4. **Card walk.** For each touched component, go through its spec card row by
   row against the render and the code. Any ✅ that is no longer true becomes
   a finding; any new element or state is added to the card as a row.
5. **Screen checklist.** DESIGN.md §5 "After implementing": the ten
   heuristics and the visual pass (alignment on one centre line in bars,
   spacing on the 4/8/12/16/24/32 scale, 32 px controls, contrast, focus ring,
   icons 16/20 px, no emoji, German fits, narrow window 900 × 560 holds).
6. **Tests.** `cargo nextest run -p clipforge-app` (includes the token lint
   and gallery-coverage tests) and `cargo clippy -p clipforge-app --all-targets -- -D warnings`.
7. **Report.** Findings as a list: severity (blocker / should / nice), file
   and line or gallery row, what the reference says, what we do, proposed
   fix. Then stop and let the user pick. After fixes, re-run from step 3.

## Severity

- **blocker**: violates a platform convention, an accessibility rule, or
  loses user work; missing card or gallery row.
- **should**: guideline deviation visible to the user (alignment, missing
  state, wording, missing tooltip).
- **nice**: polish with no functional impact.

## Do not

- Do not mark anything "looks fine" without naming the render you looked at.
- Do not fix during the review pass; report first, fix after triage.
- Do not accept a token-only change ("replaced hex with Theme.x") as a
  visual improvement; the render decides.
