# Decisions: music and titles (Milestone 5)

Written with `/ux-prepare` before implementation; `/ux-review` checks
against it.

## Task

"The user wants to put a song under the slideshow, give it an opening title
and a closing card, and label photos with a place or a date, without
leaving the editor."

## Sources read (2026-09-25)

| Source | Page | Rule taken |
|---|---|---|
| NN/G | https://www.nngroup.com/videos/bulk-actions-design-guidelines/ (M4) | bulk on the selection, clear feedback, undo |
| NN/G | https://www.nngroup.com/articles/ui-copy/ | 1–4 word verb-first labels, specific ("Fill from date", not "Auto"), ellipsis when more input follows ("Add music…"), tooltips on icons |
| NN/G | https://www.nngroup.com/articles/direct-manipulation/ (M2) | what is on the timeline can be clicked and changed in place |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/TextArea.html | multi-line input for "sizable amounts of text", visible label, help text under the field, height grows with content |
| Spectrum | https://react-spectrum.adobe.com/react-spectrum/ColorSwatchPicker.html | predefined palette → swatch picker; one selected; unique colours; localised accessible name per swatch and for the group |
| Fluent 2 | https://learn.microsoft.com/en-us/windows/apps/design/controls/listview-and-gridview | a short vertical list for text items read top to bottom; item actions next to the item; reorder by drag or keyboard |
| Fluent 2 | combo box / radio buttons (M4) | < 5 static options → radio buttons (our `ChoiceGroup`) |
| Primer | Content, Empty states (DESIGN §3) | sentence case; empty area says what it is and offers one action plus drag and drop |

## Components

| Element | Choice | Why |
|---|---|---|
| Music lane under the clips | **new** `MusicLane` (`editor.slint`), same x scale as the strip | Premiere / Clipchamp audio track; direct manipulation of "the music" as one object. Shows each song as a block with its name, repeats of a looped playlist dimmed, the fade-out as a ramp, and an empty state with "Add music…". Spec card + gallery rows. |
| Song list | **new** `SongRow` (`components.slint`): name, duration, move up, move down, remove (`IconButton`s) | Fluent list view: text items with item actions beside them. Keyboard: Tab to the buttons; up/down disabled at the ends. Spec card + gallery row. |
| Music volume, fades | existing `ValueSlider` | same as clip volume |
| Loop, duck | std `Switch` | settings that apply at once (Fluent toggle switch; `Mute` uses the same) |
| Fit slideshow to music / Add music… | existing `ActionButton` | verb-first action buttons |
| Caption text | std `TextEdit` (multi-line) | Spectrum text area: captions and titles can have a second line (the title card uses it as a subtitle). Help text under it: "A second line is shown smaller on titles." |
| Caption style | existing `ChoiceGroup` (4 options: Classic, Banner, Title, Corner) | Fluent: < 5 static options. German labels kept short (Klassisch, Band, Titel, Ecke) to fit 296 px. The core name `Headline` shows as "Title". |
| Fill from date / Fill from file name / Remove captions | existing `ActionButton` | specific verbs; remove is not destructive (undo) so no confirmation |
| Title background | **new** `SwatchGroup` (`components.slint`) | Spectrum ColorSwatchPicker: five named swatches (Black, Charcoal, Blue, Red, White), one tab stop, arrow keys, colour name as tooltip and accessible label, selected ring. Colours are tokens in `theme.slint` kept equal to `render::title_rgb` by a test. |
| Add opening title / Add closing card | existing `ActionButton` | in a "Titles" section shown when nothing is selected; also in the Edit menu |
| Title clip on the timeline | existing timeline clip, new variant | background colour instead of a thumbnail, the first text line centred, type stripe `Theme.title-clip`, footer "Title" |
| Caption badge on clips | existing `Badge` (icon `text` from Fluent) | shows which clips carry a caption without opening each one |

New components: `MusicLane`, `SongRow`, `SwatchGroup` → spec cards and
gallery rows before they appear in a screen; DESIGN §4 table rows.

## Behaviour

- **Selecting the music:** clicking the music lane (or Tab to it and
  Enter/Space) selects the music: the clip selection clears, the lane shows
  the selection ring and the inspector shows only the Music section.
  Clicking a clip, empty strip space or Escape deselects it. Delete with the
  music selected removes all songs (one undo step).
- **Adding songs:** library audio items go to the music, not the clip
  track: "Add to timeline", double-click and dragging onto the timeline all
  append audio items to the playlist, while photos and videos in the same
  selection become clips. One user action is one undo step (a `Batch` of
  `InsertClips` and `SetMusic`). "Add music…" opens a file picker for audio,
  imports the files into the library, and appends each song once it has
  been read (duration known); the status line says "Added N songs".
- **Songs not ready:** an audio item that has not been read yet is skipped
  and counted in the existing "N items were skipped (not ready yet)" status.
- **Playlist:** songs play back to back from the start; order changes with
  the up/down buttons (one undo step each), remove with ×. "Loop" repeats the
  playlist until the show ends (on by default); the music always ends with
  the show with the fade-out. "Lower under video sound" (duck, on by
  default) lowers the music while a video plays its own sound so both add
  up to full volume (video 20 % → music 80 %; video 100 % → music off;
  changed 2026-09-25 on request, was a fixed -10 dB).
- **Fit slideshow to music:** sets one duration for every photo so the show
  is as long as one pass of the playlist (videos and title cards keep their
  length). One undo step (`SetPhotoDuration`); disabled without photos or
  songs. The section reads "Music 3:12 · Slideshow 2:40" so the user sees the
  mismatch before and the match after.
- **Captions:** the Caption section applies to the selection, or to all
  clips except title cards when nothing is selected (a bulk caption must
  not overwrite a title's text; selecting a card explicitly includes it). Typing
  updates the preview on each keystroke; one editing session of the field is
  one undo step (History merges consecutive caption edits to the same
  clips). With a multi-selection whose captions differ, the field is empty
  with the placeholder "Different captions"; typing replaces all of them.
  "Fill from date" writes the capture date ("25 September 2026" /
  "25. September 2026"), "Fill from file name" the name without extension;
  clips without a date are left unchanged and counted in the status.
  "Remove captions" removes them from the targets. Each is one
  `SetCaptions` step. Style applies to the targets' existing captions and is
  remembered (for the session) as the style of captions typed or filled
  next.
- **Title cards:** "Add opening title" inserts a 4 s card with the project
  name (or "My slideshow") at the start and selects it so the text field is
  ready; "Add closing card" appends "The end". Both use the project's
  default transition. A title card is a clip: select, drag, delete, duration
  presets and transitions work as for photos; the inspector shows Duration,
  Caption (text, style) and Background, and hides framing and motion.
- **Undo:** every action above is one step (`SetMusic`, `SetCaptions`,
  `SetTitleBackground`, `SetPhotoDuration`, `InsertClips`, `Batch`).
- **Feedback (< 100 ms):** lane, clips, inspector and preview update on the
  same frame; status line for added songs, filled captions and skipped
  items; music plays in the preview immediately when playback starts.
- **Keyboard:** everything Tab-reachable. The lane is a tab stop (Enter
  selects the music). No new letter shortcuts (would collide with typing
  into the caption field; the field swallows keys while focused).
- **Empty states:** empty lane: music icon, "Drop songs here or" + "Add
  music…". Empty playlist in the Music section: "No songs yet." + "Add
  music…". Caption section with no clips: hidden (nothing to caption).
- **Errors:** a song whose file is missing plays as silence (the export
  already reports missing sources); a failed import appears in the library's
  existing error flow.
- **Wording (EN / DE):** Music / Musik, Add music… / Musik hinzufügen…,
  Loop / Wiederholen, Lower under video sound / Unter Videoton leiser,
  Fade in / Einblenden, Fade out / Ausblenden, Fit slideshow to music /
  Diashow an Musik anpassen, Caption / Beschriftung, Fill from date / Aus
  Datum füllen, Fill from file name / Aus Dateiname füllen, Remove captions /
  Beschriftungen entfernen, Add opening title / Titel am Anfang, Add closing
  card / Abspann am Ende, Background / Hintergrund.
- **Platform:** file picker via `rfd` (native on both); no other
  differences.
- **Screenshot scenes:** `populated` gets two songs (lane visible, looped)
  and a title card at the start with a caption on a photo; a new `music`
  scene has the music selected (inspector Music section); `selection`-like
  scene with a title card selected shows Caption + Background.

## Tests first (view model)

`editor_view.rs`:
- `song_blocks_follow_the_playlist_and_mark_repeats`
- `song_blocks_are_empty_without_songs`
- `caption_for_multi_selection_is_shared_or_mixed`
- `caption_from_date_formats_per_language`
- `caption_from_file_name_drops_the_extension`
- `library_ids_split_into_clips_and_songs`
- `title_background_colours_match_the_renderer` (theme tokens vs `title_rgb`)

`core::history`:
- `merged_caption_edits_undo_in_one_step`

## Implementation notes

- The caption style picker is a compact `ChoiceGroup` (8 px padding) so four
  options fit in English and German.
- The clip inspector stays in the tree (hidden) while the music inspector
  covers it, so `T` can still reach the transition picker; `T` leaves the
  music first.
- Songs picked with "Add music…" are polled in the catalogue (by path) four
  times a second until the library has read them; given up after 2 min.

## Review (`/ux-review`, 2026-09-25)

Renders looked at: `populated.png`, `music.png`, `title.png`, `narrow.png`,
`gallery.png` (M5 rows cropped at 2×). Tests: `cargo nextest run -p
clipforge-app` 58/58 including token lint and gallery coverage; clippy clean.

Fixed after triage (the user asked for all findings):

1. **should** — the German switch label "Wiederholen bis zum Ende der
   Diashow" could not wrap and pushed the Music inspector wider than
   296 px, cutting off read-outs and buttons. Label is now "Loop" /
   "Wiederholen" with a caption below; the full sentence is the accessible
   label.
2. **should** — with nothing selected, typing or filling captions also
   overwrote title cards, and "Fill from date" counted cards as missing a
   date. Bulk caption actions now skip title cards unless they are selected
   (`caption_targets`, test `bulk_captions_leave_title_cards_alone`).
3. **nice** — removing every song needed the Delete key; the Songs section
   has a "Remove all songs" button (one undo step).
4. **nice** — the Charcoal swatch barely showed on the pane; swatches get a
   light edge.
5. **nice** — the `title` scene now selects the title card.

No open findings.
