//! Editor actions for music, captions and title cards
//! (docs/ux/decisions/m5-music-titles.md). Part of `editor_ui`; works on
//! the same `Inner` state.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clipforge_core::music::playlist_length;
use clipforge_core::project::{CaptionStyle, TitleBackground};
use clipforge_core::timeline::{fit_photo_duration, total_duration};
use clipforge_core::{Clip, Command, MediaRef, Music, Song, Ticks};
use clipforge_library::ProbeState;
use clipforge_media::MediaKind;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tracing::warn;

use super::Inner;
use crate::editor_view::{self, media_ref_for};
use crate::format;
use crate::ui::{EditorState, SongBlockView, SongItem, Strings};

/// Undo group for typing into the caption field (one step per session).
const CAPTION_GROUP: u64 = 1;
/// Songs picked with "Add music…" are given up on after this long.
const PENDING_SONG_TIMEOUT: Duration = Duration::from_secs(120);
/// Length of a new title card.
const TITLE_DURATION: Ticks = Ticks::from_seconds(4);

/// Songs picked with "Add music…", waiting for the library to read them.
#[derive(Debug, Default)]
pub(super) struct PendingSongs {
    paths: Vec<PathBuf>,
    since: Option<Instant>,
    last_poll: Option<Instant>,
}

impl Inner {
    // ----- music ------------------------------------------------------------

    /// Click on the music lane: the music becomes the selection.
    pub(super) fn music_lane_pressed(&mut self) {
        if self.cancel_trim() {
            return;
        }
        self.selection.clear();
        self.music_selected = true;
        self.sync_timeline();
    }

    /// Any clip selection (or Escape) leaves the music.
    pub(super) fn leave_music(&mut self) {
        if self.music_selected {
            self.music_selected = false;
            self.history.seal();
        }
    }

    fn set_music(&mut self, music: Music, media: Vec<MediaRef>) {
        self.apply(Command::SetMusic { music, media });
    }

    pub(super) fn song_move(&mut self, index: i32, up: bool) {
        let Ok(i) = usize::try_from(index) else {
            return;
        };
        let mut music = self.project.music.clone();
        let j = if up { i.checked_sub(1) } else { Some(i + 1) };
        let Some(j) = j.filter(|j| *j < music.songs.len() && i < music.songs.len()) else {
            return;
        };
        music.songs.swap(i, j);
        self.set_music(music, Vec::new());
    }

    pub(super) fn song_remove(&mut self, index: i32) {
        let Ok(i) = usize::try_from(index) else {
            return;
        };
        let mut music = self.project.music.clone();
        if i >= music.songs.len() {
            return;
        }
        music.songs.remove(i);
        self.set_music(music, Vec::new());
    }

    /// Delete with the music selected: remove every song.
    pub(super) fn remove_all_songs(&mut self) {
        if self.project.music.songs.is_empty() {
            return;
        }
        let music = Music {
            songs: Vec::new(),
            ..self.project.music.clone()
        };
        self.set_music(music, Vec::new());
    }

    pub(super) fn music_volume(&mut self, percent: f32) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let volume_percent = percent.round().clamp(0.0, f32::from(Music::MAX_VOLUME)) as u16;
        if volume_percent == self.project.music.volume_percent {
            return;
        }
        let music = Music {
            volume_percent,
            ..self.project.music.clone()
        };
        self.set_music(music, Vec::new());
    }

    pub(super) fn music_fades(&mut self, fade_in: f32, fade_out: f32) {
        let secs = |s: f32| Ticks::from_seconds_f64(f64::from(s.max(0.0)));
        let music = Music {
            fade_in: secs(fade_in),
            fade_out: secs(fade_out),
            ..self.project.music.clone()
        };
        if music != self.project.music {
            self.set_music(music, Vec::new());
        }
    }

    pub(super) fn music_loop(&mut self, looped: bool) {
        let music = Music {
            looped,
            ..self.project.music.clone()
        };
        self.set_music(music, Vec::new());
    }

    pub(super) fn music_duck(&mut self, duck: bool) {
        let music = Music {
            duck,
            ..self.project.music.clone()
        };
        self.set_music(music, Vec::new());
    }

    /// Every photo gets the same duration so the show lasts one pass of the
    /// playlist. One undo step.
    pub(super) fn fit_to_music(&mut self) {
        let target = playlist_length(&self.project);
        let photos: Vec<usize> = (0..self.project.clips.len())
            .filter(|&i| self.project.clips[i].is_photo())
            .collect();
        if target <= Ticks::ZERO || photos.is_empty() {
            return;
        }
        let Some(duration) = fit_photo_duration(&self.project.clips, target) else {
            return;
        };
        self.apply(Command::SetPhotoDuration {
            indices: photos,
            duration,
        });
        self.preview_dirty = true;
    }

    /// Appends `refs` (audio) to the playlist, in the same undo step as
    /// `clips_command` when given.
    pub(super) fn songs_command(&self, refs: Vec<MediaRef>) -> Option<Command> {
        if refs.is_empty() {
            return None;
        }
        let mut music = self.project.music.clone();
        music.songs.extend(refs.iter().map(|r| Song::new(r.id)));
        Some(Command::SetMusic { music, media: refs })
    }

    /// "Add music…": pick audio files, import them into the library, and add
    /// each one as a song once it has been read.
    pub(super) fn add_music(&mut self) {
        let exts: Vec<&str> = MediaKind::supported_extensions()
            .filter(|e| MediaKind::from_extension(e) == MediaKind::Audio)
            .collect();
        let picked = rfd::FileDialog::new()
            .add_filter("Audio", &exts)
            .pick_files()
            .unwrap_or_default();
        if picked.is_empty() {
            return;
        }
        self.library.import(picked.clone());
        self.pending_songs.paths.extend(picked);
        self.pending_songs.since = Some(Instant::now());
        self.poll_pending_songs(true);
    }

    /// Adds picked songs that the library has finished reading. Called from
    /// the timer; polls four times a second.
    pub(super) fn poll_pending_songs(&mut self, now_please: bool) {
        if self.pending_songs.paths.is_empty() {
            return;
        }
        let now = Instant::now();
        if !now_please
            && self
                .pending_songs
                .last_poll
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(250))
        {
            return;
        }
        self.pending_songs.last_poll = Some(now);
        let mut ready = Vec::new();
        let mut failed = 0usize;
        {
            let cat = self.library.catalogue();
            self.pending_songs
                .paths
                .retain(|path| match cat.find_by_path(path).ok().flatten() {
                    Some(record) if record.probe == ProbeState::Done => {
                        match media_ref_for(&record) {
                            Some(r) if r.kind == clipforge_core::RefKind::Audio => ready.push(r),
                            _ => failed += 1,
                        }
                        false
                    }
                    Some(record) if matches!(record.probe, ProbeState::Failed(_)) => {
                        failed += 1;
                        false
                    }
                    _ => true,
                });
        }
        if self
            .pending_songs
            .since
            .is_some_and(|t| now.duration_since(t) > PENDING_SONG_TIMEOUT)
        {
            failed += self.pending_songs.paths.len();
            self.pending_songs.paths.clear();
        }
        let added = ready.len();
        if let Some(cmd) = self.songs_command(ready) {
            self.apply(cmd);
            self.status_text(|s| {
                s.set_count(count(added));
                s.get_added_songs()
            });
        } else if failed > 0 {
            self.status_text(|s| {
                s.set_count(count(failed));
                s.get_skipped_items()
            });
        }
    }

    // ----- captions ---------------------------------------------------------

    /// What the caption controls act on (never title cards in bulk).
    fn caption_targets(&self) -> Vec<usize> {
        editor_view::caption_targets(&self.project, &self.selection.indices(&self.project.clips))
    }

    /// Typing into the caption field; one undo step per editing session.
    pub(super) fn caption_edited(&mut self, text: &str) {
        let targets = self.caption_targets();
        if targets.is_empty() {
            return;
        }
        let entries =
            editor_view::caption_text_entries(&self.project, &targets, text, self.caption_style);
        match self.history.apply_merging(
            &mut self.project,
            Command::SetCaptions { entries },
            CAPTION_GROUP,
        ) {
            Ok(()) => {
                self.dirty_since.get_or_insert_with(Instant::now);
                self.preview_dirty = true;
                self.sync_all();
            }
            Err(e) => warn!(error = %e, "caption rejected"),
        }
    }

    /// The caption field lost focus: the next edit is a new undo step.
    pub(super) fn caption_committed(&mut self) {
        self.history.seal();
    }

    pub(super) fn caption_style_changed(&mut self, index: i32) {
        self.caption_style = CaptionStyle::from_index(usize::try_from(index).unwrap_or(0));
        let entries = editor_view::caption_style_entries(
            &self.project,
            &self.caption_targets(),
            self.caption_style,
        );
        if entries.is_empty() {
            self.sync_inspector();
            return;
        }
        self.apply(Command::SetCaptions { entries });
        self.preview_dirty = true;
    }

    pub(super) fn caption_fill_date(&mut self) {
        let Some(w) = self.state() else { return };
        let strings = w.global::<Strings>();
        let months: Vec<String> = strings
            .get_month_names()
            .split(',')
            .map(|m| m.trim().to_owned())
            .collect();
        let (entries, skipped) = editor_view::caption_fill_entries(
            &self.project,
            &self.caption_targets(),
            self.caption_style,
            |m| {
                let (d, month, y) = format::civil_date(m.captured_at_ms?);
                strings.set_date_day(d.to_string().into());
                strings.set_date_month(
                    months
                        .get(month as usize - 1)
                        .cloned()
                        .unwrap_or_else(|| month.to_string())
                        .into(),
                );
                strings.set_date_year(y.to_string().into());
                Some(strings.get_caption_date().to_string())
            },
        );
        self.apply_fill(entries, skipped);
    }

    pub(super) fn caption_fill_name(&mut self) {
        let (entries, skipped) = editor_view::caption_fill_entries(
            &self.project,
            &self.caption_targets(),
            self.caption_style,
            |m| Some(editor_view::caption_from_file_name(&m.name)),
        );
        self.apply_fill(entries, skipped);
    }

    fn apply_fill(
        &mut self,
        entries: Vec<(usize, Option<clipforge_core::project::Caption>)>,
        skipped: usize,
    ) {
        let filled = entries.len();
        if filled > 0 {
            self.apply(Command::SetCaptions { entries });
            self.preview_dirty = true;
        }
        self.status_text(|s| {
            if skipped > 0 {
                s.set_count(count(skipped));
                s.get_captions_skipped()
            } else {
                s.set_count(count(filled));
                s.get_filled_captions()
            }
        });
    }

    pub(super) fn caption_remove(&mut self) {
        let entries: Vec<_> = self
            .caption_targets()
            .into_iter()
            .filter(|&i| self.project.clips[i].caption.is_some())
            .map(|i| (i, None))
            .collect();
        if entries.is_empty() {
            return;
        }
        self.apply(Command::SetCaptions { entries });
        self.preview_dirty = true;
    }

    // ----- title cards ------------------------------------------------------

    pub(super) fn title_background_changed(&mut self, index: i32) {
        let background = TitleBackground::from_index(usize::try_from(index).unwrap_or(0));
        let indices: Vec<usize> = self
            .targets()
            .into_iter()
            .filter(|&i| self.project.clips[i].is_title())
            .collect();
        if indices.is_empty() {
            return;
        }
        self.apply(Command::SetTitleBackground {
            indices,
            background,
        });
        self.preview_dirty = true;
    }

    pub(super) fn add_title(&mut self, closing: bool) {
        let Some(w) = self.state() else { return };
        let strings = w.global::<Strings>();
        let text = if closing {
            strings.get_closing_title().to_string()
        } else {
            self.project_path
                .as_ref()
                .and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .map(|s| s.trim_end_matches(".clipforge").to_owned())
                .unwrap_or_else(|| strings.get_opening_title().to_string())
        };
        let mut clip = Clip::title(text, TitleBackground::Black, TITLE_DURATION);
        clip.transition_in = self.project.settings.default_transition;
        let at = if closing { self.project.clips.len() } else { 0 };
        let id = clip.id;
        self.leave_music();
        self.apply(Command::InsertClips {
            entries: vec![(at, clip)],
            media: Vec::new(),
        });
        // Select the new card so its text is ready to edit.
        if let Some(index) = self.project.index_of(id) {
            self.selection
                .click(&self.project.clips, index, false, false);
            if let Some(p) = clipforge_core::timeline::placements(&self.project.clips).get(index) {
                self.playhead = p.start;
            }
        }
        self.preview_dirty = true;
        self.sync_timeline();
    }

    // ----- sync -------------------------------------------------------------

    /// Music lane and Music inspector.
    pub(super) fn sync_music(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let clips = self.display_clips();
        let boxes = self.display_boxes();
        let lane = editor_view::music_lane(&self.project, &clips, &boxes);
        let blocks: Vec<SongBlockView> = lane
            .blocks
            .iter()
            .map(|b| SongBlockView {
                x: b.x,
                width: b.width,
                title: SharedString::from(b.title.as_str()),
                repeat: b.repeat,
            })
            .collect();
        s.set_song_blocks(ModelRc::new(VecModel::from(blocks)));
        s.set_music_fade_x(lane.fade_x);
        s.set_music_end_x(lane.end_x);
        s.set_music_selected(self.music_selected);
        let music = &self.project.music;
        let songs: Vec<SongItem> = music
            .songs
            .iter()
            .map(|song| {
                let m = self.project.media_ref(song.media);
                SongItem {
                    title: SharedString::from(m.map(|m| m.name.as_str()).unwrap_or_default()),
                    duration_text: SharedString::from(
                        m.and_then(|m| m.duration)
                            .map(format::duration)
                            .unwrap_or_default(),
                    ),
                }
            })
            .collect();
        s.set_songs(ModelRc::new(VecModel::from(songs)));
        s.set_music_volume(f32::from(music.volume_percent));
        #[allow(clippy::cast_possible_truncation)]
        {
            s.set_music_fade_in(music.fade_in.as_seconds_f64() as f32);
            s.set_music_fade_out(music.fade_out.as_seconds_f64() as f32);
        }
        s.set_music_loop(music.looped);
        s.set_music_duck(music.duck);
        let playlist = playlist_length(&self.project);
        s.set_music_length_text(format::duration(playlist).into());
        s.set_show_length_text(format::duration(total_duration(&self.project.clips)).into());
        s.set_can_fit_music(
            playlist > Ticks::ZERO && self.project.clips.iter().any(Clip::is_photo),
        );
    }

    /// Caption and title-card controls in the clip inspector.
    pub(super) fn sync_captions(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let targets = self.caption_targets();
        let all = self.targets();
        let view = editor_view::caption_view(&self.project, &targets, self.caption_style);
        if s.get_caption_text() != view.text.as_str() {
            s.set_caption_text(view.text.into());
        }
        s.set_caption_mixed(view.mixed);
        s.set_caption_any(view.any);
        s.set_caption_style_index(i32::try_from(view.style.index()).unwrap_or(0));
        let titles: Vec<&Clip> = all
            .iter()
            .map(|&i| &self.project.clips[i])
            .filter(|c| c.is_title())
            .collect();
        s.set_has_title_target(!titles.is_empty());
        s.set_only_title_target(!titles.is_empty() && titles.len() == all.len());
        if let Some(clipforge_core::ClipSource::Title { background, .. }) =
            titles.first().map(|c| &c.source)
        {
            s.set_title_background_index(i32::try_from(background.index()).unwrap_or(0));
        }
    }

    /// Shows a transient status line built from the `Strings` global.
    pub(super) fn status_text(&mut self, build: impl FnOnce(&Strings<'_>) -> SharedString) {
        let Some(w) = self.state() else { return };
        let text = build(&w.global::<Strings>());
        self.status_since = Some(Instant::now());
        w.global::<EditorState>().set_status_text(text);
    }
}

fn count(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}
