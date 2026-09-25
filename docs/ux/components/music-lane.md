# MusicLane (spec card)

Status: draft 2026-09-25
Implementation: `crates/app/ui/editor.slint` → `MusicLane` (inside `TimelineStrip`)
Gallery rows: "MusicLane (40 px): songs with a looped repeat and fade-out; selected + keyboard focus; empty; songs but no clips"

## References (read, not remembered)

| Source | Page | Owns |
|---|---|---|
| NN/G | https://www.nngroup.com/articles/direct-manipulation/ | the music is a visible object on the timeline that is selected in place |
| Fluent 2 | https://learn.microsoft.com/en-us/windows/apps/design/controls/listview-and-gridview | selection by click, Space/Enter; focus visible |
| Primer | Empty states (DESIGN §3) | say what the area is and offer one action plus drag and drop |
| Premiere / Clipchamp | audio track under the video track (convention) | songs as blocks on the same time scale as the clips |

## Anatomy

| Element | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| Track container | audio track under the video track | 40 px (`bar-height`) lane under the clips, `surface-pane`, `radius-m`, 1 px border | ✅ | gallery "MusicLane" |
| Song block | clip-like block per audio item, same time scale | block per `SongSpan`, x from `x_at_time` (follows the strip's minimum clip widths), `Theme.audio` at 45 % | ✅ | `editor_view::music_lane` tests |
| Song name | label on the block | music icon 16 px + caption 11 px, elided | ✅ | gallery |
| Looped repeat | — (ours) | same block at 22 %, so the first pass reads as "the playlist" | ✅ | gallery row 1 |
| Fade-out | waveform/ramp in editors | gradient to `surface-window` from fade start to music end | ✅ | gallery rows 1–2 |
| Empty state | Primer: what + one action + drag hint | icon, "Drop songs here or", `ActionButton` "Add music…" | ✅ | gallery row 3 |
| Songs but no clips | — | caption "{n} songs play under the slideshow once it has clips" | ✅ | gallery row 4 |

## States

| State | Reference says | Ours | Status | Evidence |
|---|---|---|---|---|
| default | | as above | ✅ | gallery |
| hover | Spectrum cards: hover tint | — no hover tint (the whole lane is one target; the cursor stays default) | ➖ | |
| selected | selection colour ring + tint (DESIGN §3) | `selection-tint` background, 2 px `selection` border | ✅ | gallery row 2 |
| focus (keyboard) | focus visible | 2 px `focus-ring` inset, only after keyboard focus | ✅ | gallery row 2 |
| drag over | drop zone tint | the strip's existing drop overlay covers the lane | ✅ | `TimelineStrip` |

## Metrics

| Property | Reference | Ours | Status |
|---|---|---|---|
| height | dense rows 40 px (Fluent command bar dense) | 40 px, blocks 32 px | ✅ |
| gap to clips | 4/8 scale | 4 px (`space-xs`) | ✅ |
| radius | cards `radius-m`, blocks `radius-s` | same | ✅ |
| icon size | 16 px | 16 px | ✅ |

## Behaviour and keyboard

| Rule | Reference | Ours | Status | Evidence |
|---|---|---|---|---|
| click selects the music, clears the clip selection | DESIGN §3 selection | `music-lane-pressed` → `music_selected` | ✅ | manual-checks M5 |
| Tab reaches the lane; Enter/Space selects | Fluent list keyboard | `lane-focus` FocusScope | ✅ | keyboard.md |
| Escape / clip click / empty strip click leaves the music | DESIGN §3 | `leave_music` in `clear_selection`, `clip_pressed`, `scrub` | ✅ | manual-checks M5 |
| Delete removes all songs (one undo step) | undo everything | `remove_all_songs` → `SetMusic` | ✅ | manual-checks M5 |
| dropping audio from the library adds songs | NN/G drag and drop | `insert_media` splits audio into `SetMusic` | ✅ | `library_ids_split_into_clips_and_songs` |

## Accessibility

| Rule | Reference | Ours | Status |
|---|---|---|---|
| role | button (activates a selection) | `accessible-role: button` | ✅ |
| label | name + state | "Music: {n} songs" / "Music: no songs" | ✅ |
| contrast | UI 3:1 | audio green blocks on pane; text white on 45 % green | ✅ |
| focus visible | | ring | ✅ |

## Wording

"Drop songs here or" + "Add music…" (ellipsis: opens a file picker), "Music:
{} songs", "{} songs play under the slideshow once it has clips". German:
"Musik hier ablegen oder", "Musik hinzufügen…", "Musikstücke" (not "Titel",
which collides with title cards).

## Gaps

None open.
