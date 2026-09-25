# Decisions: text track and on-preview editing (M5, revised)

Written with `/ux-prepare` before implementation; `/ux-review` checks
against it. Replaces the per-clip captions of `m5-music-titles.md`
(Captions section, caption styles, fill from date / file name) on the
user's request (2026-09-25): "a separate track on top of the videos/images
where I can place text … editable in a WYSIWYG way on the preview,
including moving it around, selecting different styles, fonts, etc.
Blending like for videos/images should also be available."

## Task

"The user wants to put text anywhere in the slideshow — any time, any place
in the frame, any look — and adjust it by pointing at it in the preview."

## Sources read (2026-09-25)

| Source | Page | Rule taken |
|---|---|---|
| NN/G | https://www.nngroup.com/articles/direct-manipulation/ | continuous representation while dragging; visible handles large enough to hit; keyboard alternatives for precision; reversible, incremental actions |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ActionGroup.html | text formatting (bold, italic) as a multiple-selection group of icon buttons with tooltips; alignment as single selection; one tab stop, arrow keys |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ColorSwatchPicker.html (M5) | predefined palette with named swatches |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/Picker.html (M4) | picker for a list of ≥ 5 single-line choices (fonts, animations) with a label |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/TextArea.html (M5) | multi-line field with a visible label |
| Fluent 2 | keyboard interactions (M3 card) | arrows nudge the focused object, Escape cancels a gesture, Delete removes |
| Premiere / Clipchamp | text track above the video track (convention) | texts are timeline items with their own start and length |

## Model (core)

`Project.texts: Vec<TextItem>`; later items draw on top. A `TextItem` has
text, start, duration, centre `x, y` and wrap `width` (1/10 000 of the
frame), a `TextStyle` (font from six bundled OFL fonts, size relative to the
short side, bold, italic, colour, alignment, optional background box,
shadow) and `enter` / `exit` animations (cut, fade, slide ×4, wipe ×4, zoom
— the clip transitions applied to the text). Commands: `InsertTexts`,
`RemoveTexts`, `SetTexts` (any number of items, one undo step). Old
per-clip captions are converted to text items when a project loads. Title
cards become plain colour cards; the title text lives on the text track.

## Components

| Element | Choice | Why |
|---|---|---|
| Text lane above the clips | **new** `TextLane` (`editor.slint`): 28 px rows, overlapping items stack into up to three rows | Premiere convention; direct manipulation in time (drag to move, edges to trim), same x scale as the clips. Spec card + gallery rows. |
| On-preview editing | **new** `TextOverlay` (`editor.slint`): hit areas for visible texts, selection box with four corner handles (size) and two side handles (wrap width), centre snap guides, inline text field | NN/G direct manipulation; Canva/Keynote convention. Spec card + gallery row. |
| Text content | std `TextEdit` in the inspector, and the inline field on the preview | two paths to the same value |
| Font | std `ComboBox` (6 fonts) | ≥ 5 options → picker |
| Size | existing `ValueSlider` (1 – 30 % of the short side) | |
| Bold / Italic | existing `IconButton` (checkable) with new Fluent icons `text_bold`, `text_italic`, tooltips | Spectrum ActionGroup, multiple selection |
| Alignment | existing `ChoiceGroup` (Left / Centre / Right) | Spectrum ActionGroup, single selection |
| Colour, box colour | existing `SwatchGroup` (8 named colours) | predefined palette |
| Box behind text, shadow | std `Switch` | immediate on/off |
| Timing | existing `ValueSlider` (duration) | |
| Entrance / exit | std `ComboBox` (11 movements) + `ValueSlider` (duration) each | same vocabulary as clip transitions |
| Add text | `ActionButton` "Text" in the editor toolbar, Edit menu, Ctrl/Cmd+T, double-click on the empty text lane | recognition (button, menu) and efficiency (shortcut, double-click) |

## Behaviour

- **Selection:** texts, clips and the music are separate selections; picking
  one clears the others. Click a text block on the lane or a visible text on
  the preview to select it; Shift / Ctrl / Cmd-click adds on the lane.
  Clicking empty preview space or Escape clears the text selection.
- **Moving on the preview:** drag the text; it follows the pointer
  (continuous feedback, rendered by the real compositor); near the
  horizontal or vertical centre it snaps and a guide line shows. One undo
  step on release; Escape during the drag puts it back.
- **Resizing on the preview:** side handles change the wrap width around the
  centre; corner handles scale the font size (and the width with it). One
  undo step on release.
- **Editing in place:** double-click a text on the preview (or Enter with a
  text selected) opens a text field over it with the same font, size,
  colour and alignment; the rendered text hides meanwhile. Escape, a click
  outside or focus leaving ends editing. A typing session is one undo step.
- **Keyboard:** with a text selected and the preview focused, arrows nudge by
  0.5 % of the frame (Shift: 5 %); Delete removes the selected texts;
  Ctrl/Cmd+T adds a text at the playhead.
- **Timing on the lane:** drag a block to move it in time; drag its edges to
  change start or end (at least 0.2 s); one undo step each. Double-click on
  empty lane space adds a 4 s text there.
- **New text:** "Your text" at the playhead (or 0), 4 s, centred, default
  style, fade in and out; selected, and the inline editor opens so typing
  replaces the placeholder.
- **Bulk:** inspector controls apply to all selected texts in one step; a
  multi-selection with different values shows the first item's value; the
  text field shows "Different texts" when they differ.
- **Titles:** "Add opening title" inserts a black colour card at the start
  and a large text over it; "Add closing card" does the same at the end.
- **Feedback (< 100 ms):** lane, overlay and preview update together;
  status line "Added a text" is not needed (the text appears selected).
- **Empty state:** empty lane row shows "Double-click to add text" in the
  lane; the toolbar button is always there.
- **Errors:** none from the user's side; invalid values are clamped.
- **Wording (EN / DE):** Text / Text, Add text / Text hinzufügen, Your text /
  Dein Text, Font / Schrift, Size / Größe, Bold / Fett, Italic / Kursiv,
  Colour / Farbe, Box behind text / Kasten hinter dem Text, Shadow /
  Schatten, Entrance / Einblenden, Exit / Ausblenden, Different texts /
  Unterschiedliche Texte, Double-click to add text / Doppelklicken, um Text
  hinzuzufügen.
- **Platform:** Ctrl+T on Windows, Cmd+T on macOS; otherwise identical.
- **Screenshot scenes:** `populated` gets two texts on the lane and one
  visible on the preview; a `text` scene has a text selected (handles on
  the preview, Text inspector); `text-edit` shows the inline editor.

## Tests first (view model, `editor_view.rs` / `text_view.rs`)

- `text_rows_stack_overlapping_items`
- `text_blocks_follow_the_strip_scale`
- `drag_moves_by_the_pointer_delta_and_snaps_to_centre`
- `side_handle_changes_width_around_the_centre`
- `corner_handle_scales_size_and_width`
- `lane_drag_moves_and_trims_with_a_minimum_length`
- `new_text_starts_at_the_playhead`
