---
name: ux-prepare
description: The before-coding ritual for a ClipForge UI feature - name the user task, look up the owning design-system pages and NN/G article, write down the behaviour decisions (selection, undo, feedback, keyboard, empty and error states, wording) and the components to reuse, so the implementation and the later ux-review have a written target. Use before starting any feature that the user sees - "prepare <feature>", "before we build X", "what do the guidelines say about X".
---

# ux-prepare — decide before you draw

AGENTS.md rule 13: look up first, write the decisions down, then code. This
skill produces that note. It takes ten minutes and saves the rework of
building the wrong thing neatly.

## Steps

1. **Task statement.** One sentence, user's words: "The user wants to …".
2. **Find the topic** in `docs/ux/DESIGN.md` §1 quick links. Fetch the NN/G
   article and the Fluent / Spectrum / Primer pages it names (WebFetch; use
   the react-spectrum.adobe.com mirror for Spectrum). Skim, do not summarise
   the whole page; extract the rules that constrain *this* feature.
3. **Components.** For each UI element the feature needs, pick in order:
   existing ClipForge component (DESIGN.md §4 table, `components.slint`),
   std widget, then a new component. A new component means a spec card
   (`ux-component-spec`) and a gallery row before it is used in a screen.
4. **Write the decisions** as a short note (in the PR description, the task
   notes, or `docs/ux/decisions/<feature>.md` when the feature is large):
   - selection model and bulk behaviour
   - undo: which `Command` in `core`, one step per user action
   - feedback: what changes within 100 ms; progress if > 1 s; cancel if > 10 s
   - keyboard path and shortcut (add to `docs/ux/keyboard.md`)
   - empty state, error state, wording (sentence case, verb-first, ellipsis
     on dialog-opening actions), German length
   - platform differences (Windows first, macOS deviations)
   - which screenshot scene will show it (add one if none does)
5. **Tests to write first** for the view model: list them by name.
6. Return the note. Implementation follows it; `ux-review` checks against it.
