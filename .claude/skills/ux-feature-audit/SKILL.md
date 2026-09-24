---
name: ux-feature-audit
description: Feature-level usability audit for one ClipForge area (library import and browsing, selection and drag to timeline, timeline editing and trimming, inspector, playback and scrubbing, export, settings and project files). Judges the user task against DESIGN.md §3 behaviour rules, the platform conventions and the NN/G heuristics and topic articles, and writes findings with severity to docs/ux/AUDIT-<date>.md. Use for "audit the timeline", "is the export flow good UX", "usability review of <area>", "feature audit".
---

# ux-feature-audit — is the *behaviour* right?

Components can be pixel-perfect and the feature still wrong. This skill
looks at one area as a user task, end to end, and writes down what fails.
It does not fix; fixes are separate commits per area.

## Steps

1. **Task statement.** One sentence: who does what, why, with what
   material ("A first-time user wants to turn a folder of 800 holiday
   photos and 20 videos into a video they can send to family").
2. **Walk the path** in the code and the renders. List every step the user
   takes, what they see, what feedback they get, how they undo it, how they
   do it by keyboard, and what happens on error or with empty input. Use
   `cargo run -p clipforge-app --bin screenshot` scenes; add a scene to
   `crates/app/src/bin/screenshot.rs` if the state you need is not shown.
3. **Sources.** Read (WebFetch) the pages the area needs, from DESIGN.md §1
   quick links: NN/G article for the interaction (drag and drop, bulk
   actions, direct manipulation, visibility of system status, error
   messages, empty states), the Fluent page for the control pattern, the
   Spectrum page for the editor convention, Primer for the workflow pattern
   and wording. Note the URLs in the audit.
4. **Judge** every step against:
   - DESIGN.md §3 behaviour foundations (direct manipulation, undo
     everything, selection model, bulk actions, feedback ≤ 100 ms, progress
     > 1 s, cancellable > 10 s, dialogs rare, empty states, errors, keyboard,
     wording).
   - The ten NN/G heuristics, each with a concrete observation, not a tick.
   - Platform conventions (Windows first; note macOS deviations).
5. **Write** `docs/ux/AUDIT-<YYYY-MM>.md`, one section per area, appending
   if the file exists:
   - task statement, sources read
   - findings table: id, severity (blocker / should / nice), heuristic or
     rule violated, observation (what the user experiences), proposed fix,
     scope (slint / view model / core command)
   - what is good and must be kept (so fixes do not regress it)
6. **Return** the findings table. Do not fix in this skill.

## Severity

- **blocker**: user work can be lost, no undo for an edit, no way to cancel
  long work, an action reachable only by mouse, or a platform convention
  broken (button order, shortcut, menu placement).
- **should**: missing feedback, unclear wording, hidden affordance, no empty
  or error state, inconsistency with a sibling feature.
- **nice**: polish.

## Areas and their first sources

| Area | Read first |
|---|---|
| Library import and browsing | NN/G visibility of system status; Primer loading, empty states; Fluent GridView |
| Selection and drag to timeline | NN/G drag and drop; NN/G bulk actions; Fluent drag and drop |
| Timeline editing and trimming | NN/G direct manipulation; Spectrum Slider; Apple HIG "Playing video" |
| Inspector | Spectrum Form; Primer forms; Fluent text controls |
| Playback and scrubbing | Apple HIG "Playing video"; keyboard map `docs/ux/keyboard.md` |
| Export | Primer saving, notification messaging; Fluent dialogs, progress controls |
| Settings and project files | Fluent settings page; Primer forms; Apple HIG preferences |
