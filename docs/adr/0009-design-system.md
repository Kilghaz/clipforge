# ADR-0009: Layered design guidance instead of a home-grown look

Date: 2026-09-23
Status: Accepted

## Context

The first milestones produced a working UI with ad-hoc colours, emoji glyphs
as icons and no shared rules for spacing, states or behaviour. The product
plan puts usability first (PLAN.md §1), the target users are not technical,
and the primary platform is Windows with macOS as the development platform.
Slint already renders platform-native widgets (`fluent` on Windows,
`cupertino` on macOS, ADR-0001), so any design system we adopt must sit on
top of those widgets rather than replace them.

We evaluated GitHub Primer, Adobe Spectrum, Microsoft Fluent 2 / Windows app
design, Apple's macOS Human Interface Guidelines, GNOME HIG, Material 3 and
the Nielsen Norman Group's usability material.

- Primer has the best written workflow patterns (saving, loading, empty
  states, notifications, progressive disclosure) and content guidelines, but
  it is a web system built around GitHub's brand and has nothing to say about
  menus, inspectors, drag and drop, selection or timelines.
- Spectrum is the only mature system written for editing tools (Premiere,
  Lightroom): dense panels, sliders, media grids, dark surfaces, a spacing and
  sizing scale, and component state definitions.
- Fluent 2 and the macOS HIG define what users of each platform expect from
  menus, dialogs, keyboard and drag and drop, and match the Slint styles.
- Material 3, Carbon and Atlassian are mobile- or web-first and were dropped.
- NN/G's heuristics and topic articles are the widely shared vocabulary for
  judging usability and give us a checklist that does not depend on taste.

## Decision

- Design guidance is layered, with a fixed owner per question, documented in
  `docs/ux/DESIGN.md`:
  1. **Platform behaviour**: Fluent 2 / Windows app design first, macOS HIG
     second. Behaviour follows the platform the app runs on.
  2. **Editor components and visuals**: Adobe Spectrum.
  3. **Workflow patterns and UI text**: Primer's Patterns, Content and
     Accessibility sections only; none of its colours, type or components.
  4. **Usability**: NN/G heuristics and topic articles, applied as a written
     check before and after every feature.
- **Dark only.** The app forces Slint's dark colour scheme and ships no light
  theme. Colours come from Slint's `Palette` globals plus a small set of
  semantic tokens in `ui/theme.slint`.
- **Component order**: Slint `std-widgets` first, then a custom component
  modelled on the Spectrum or Fluent definition, then a new design, in that
  order and documented.
- **Icons**: Fluent UI System Icons (MIT), a curated subset fetched by an
  xtask step. No emoji as icons.
- Every feature is looked up in the sources before design and passed
  through the usability and visual checklist after implementation; this is
  part of the definition of done in `CLAUDE.md`.

## Consequences

- Fewer taste debates: a question about spacing, a state or a dialog has a
  source to cite.
- Custom Slint components need to reproduce the states their reference
  defines (hover, down, focus, disabled, selected, drag-over), which is more
  work per component but keeps the app coherent.
- Adding a light theme later means a new ADR and a review of every semantic
  token.
- The three external systems evolve independently; `DESIGN.md` pins what we
  took from each, so a change upstream does not silently change our rules.
- Existing UI (library grid, timeline, settings) does not yet follow this
  guide; it is brought into line feature by feature, not in one big pass.
