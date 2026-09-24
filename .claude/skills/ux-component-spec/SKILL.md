---
name: ux-component-spec
description: Write or refresh the spec card for one ClipForge UI component from the actual design-system pages (Fluent 2, Spectrum, Primer, NN/G), then diff the card against the Slint implementation and the gallery and return the gap list. Use when a component is created, touched, or when someone asks "does X follow the style guide". Trigger phrases - "spec card for", "does the <component> match the guidelines", "audit the <component>", "component spec".
---

# ux-component-spec — one component, from the sources

The failure this skill prevents: judging a component against a *memory* of the
guidelines. The search field shipped without icon, clear button or Escape
because nobody opened the Spectrum page. So: fetch first, write the card,
then compare. Never fill a card from recollection.

## Inputs

- Component name (a `.slint` component in `crates/app/ui/`, or a std widget we
  use: ComboBox, CheckBox, Slider, ProgressIndicator, MenuBar).
- `docs/ux/DESIGN.md` §1 for which source owns which question.
- `docs/ux/components/TEMPLATE.md` for the card format.

## Steps

1. **Locate.** Read the component's Slint source and every gallery row that
   shows it (`crates/app/ui/gallery.slint`). Note the props and states it has.
2. **Fetch the references.** Use WebFetch on the real pages; if a page is
   JS-rendered and empty, use the alternative listed below. Record the URL you
   actually read in the card. Minimum: the Fluent page and the Spectrum page.
   Add Primer for wording/patterns, NN/G for the interaction rule, Apple HIG
   only where macOS differs.
   - Fluent 2 / Windows controls: `https://learn.microsoft.com/en-us/windows/apps/design/controls/<control>`
     (buttons, command-bar, text-box, auto-suggest-box, slider, checkbox,
     combo-box, dialogs-and-flyouts/dialogs, tooltips, progress-controls,
     info-badge, list-view-and-grid-view, menus).
   - Spectrum: `https://spectrum.adobe.com/page/<component>/` is JS-rendered;
     use `https://react-spectrum.adobe.com/react-spectrum/<Component>.html`
     (Button, ActionButton, ActionGroup, SearchField, TextField, Slider,
     Badge, StatusLight, IllustratedMessage, Dialog, Tooltip, ProgressBar,
     ListView, Card) which documents the same anatomy, states and keyboard.
   - Primer patterns: `https://primer.style/product/ui-patterns/<pattern>/`
     (empty-states, loading, saving, notification-messaging, forms,
     progressive-disclosure) and `https://primer.style/product/ui-patterns/button-usage/`.
   - NN/G: search `site:nngroup.com <topic>`.
3. **Write the card** at `docs/ux/components/<kebab-name>.md` from the
   template. Every element, state, metric and behaviour rule the references
   define becomes a row, even the ones we lack. Fill "Ours" from the code, not
   from the render. Fill "Evidence" with a gallery row title or a test name.
4. **Render and compare.** `cargo run -p clipforge-app --bin screenshot -- gallery`
   and look at the component's rows at 2×. Mark each row ✅ / ❌ / ➖ / 🔒.
   A missing gallery row for a state that exists is itself a ❌.
5. **Gap list.** Fill the "Gaps" section: severity, fix or accepted deviation.
   Return the gap list as your result. Do **not** fix gaps inside this skill;
   `ux-review` or the feature work does that, so the card stays the record of
   what was found.

## Rules

- A std widget row about radius, font or colour is 🔒 when the Slint style
  owns it (fluent 4 px radius matches ours; cupertino rounds more). Anything
  behavioural about a std widget (keyboard, states we drive) is still ours.
- ➖ needs a reason in the row. "Not needed" without a reason is a ❌.
- Keep cards factual and short; the references are linked, not copied.
- Status line at the top: `audited YYYY-MM-DD` only when every row has a
  status and the Gaps section is current.
