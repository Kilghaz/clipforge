//! Glue between the Slint `EditorState` global and the core project model.
//!
//! Everything runs on the UI thread except export, which is a job. The
//! project is mutated only through `History::apply`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clipforge_core::project::{Transition, TransitionKind};
use clipforge_core::timeline::total_duration;
use clipforge_core::{Aspect, Command, Fit, History, MediaId, Project, ProjectSettings, Ticks};
use clipforge_export::{EncodePlan, ExportOptions, Exporter, FileSources, TimelineFrames};
use clipforge_jobs::{CancellationToken, JobError, Priority, Scheduler};
use clipforge_library::{Library, ThumbLevel};
use clipforge_media::{Backends, FfmpegCli, FfmpegLocation, MediaInfo, Prober};
use clipforge_platform::AppDirs;
use clipforge_render::{Frame, RenderQuality, SourceImage, SourceProvider};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use tracing::{info, warn};

use crate::editor_view::{
    self, Nudge, Selection, clip_nav, clips_for, layout, media_ref_for, nudge_target,
};
use crate::export_view;
use crate::format;
use crate::player::Player;
use crate::preview_worker::PreviewWorker;
use crate::ui::{EditorState, MainWindow, TimelineClip};

mod music_titles;
mod texts;

const AUTOSAVE_DELAY: Duration = Duration::from_secs(3);
/// How long a status message stays in the transport bar.
const STATUS_VISIBLE: Duration = Duration::from_secs(5);
const PREVIEW_MIN_INTERVAL: Duration = Duration::from_millis(33);

pub(crate) struct EditorController {
    inner: Rc<RefCell<Inner>>,
    _timer: Timer,
}

/// Preview pixels: stills from the library's 1280 px thumbnails, video
/// frames from the player's background fetchers. Shared with the preview
/// worker thread, hence the mutexes.
struct PreviewSources {
    images: Mutex<HashMap<MediaId, SourceImage>>,
    missing: Mutex<HashSet<MediaId>>,
    player: Arc<Player>,
    /// Path and probe info per video, for opening fetchers.
    videos: Mutex<HashMap<MediaId, (PathBuf, MediaInfo)>>,
    backends: Backends,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl SourceProvider for PreviewSources {
    fn still(&self, media: MediaId, _max_edge: u32) -> Option<SourceImage> {
        let hit = lock(&self.images).get(&media).cloned();
        if hit.is_none() {
            lock(&self.missing).insert(media);
        }
        hit
    }

    fn video_frame(
        &self,
        media: MediaId,
        source_time: Ticks,
        _max_edge: u32,
    ) -> Option<SourceImage> {
        self.player.video_frame(media, source_time, || {
            let videos = lock(&self.videos);
            let (path, info) = videos.get(&media)?;
            Some((path.clone(), info.clone()))
        })
    }
}

impl PreviewSources {
    /// Makes sure the player can open `media` (probes once).
    fn register_video(&self, media: MediaId, path: &std::path::Path) {
        if lock(&self.videos).contains_key(&media) {
            return;
        }
        if let Ok(info) = self.backends.probe(path) {
            lock(&self.videos).insert(media, (path.to_path_buf(), info));
        }
    }

    /// Ids the last render could not draw; cleared for the next render.
    fn take_missing(&self) -> Vec<MediaId> {
        lock(&self.missing).drain().collect()
    }
}

/// A trim drag: where it started and the values under the cursor now.
#[derive(Copy, Clone, Debug)]
struct TrimState {
    anchor: editor_view::TrimAnchor,
    in_point: Ticks,
    out_point: Ticks,
    /// Escape was pressed; ignore further moves until the button is released.
    cancelled: bool,
}

enum ExportEvent {
    Progress(f32),
    /// The report and what ffprobe found wrong with the file (empty: fine).
    Finished(Result<(clipforge_export::ExportReport, Vec<String>), String>),
}

struct ExportRun {
    token: CancellationToken,
    events: crossbeam_channel::Receiver<ExportEvent>,
    started: std::time::Instant,
}

struct Inner {
    window: slint::Weak<MainWindow>,
    library: Arc<Library>,
    scheduler: Arc<Scheduler>,
    backends: Backends,
    dirs: AppDirs,
    project: Project,
    history: History,
    selection: Selection,
    project_path: Option<PathBuf>,
    playhead: Ticks,
    playing: bool,
    last_tick: Instant,
    pps: f32,
    preview: Arc<PreviewSources>,
    worker: PreviewWorker,
    /// Immutable copy of the project handed to the preview worker; refreshed
    /// after every change.
    snapshot: Arc<Project>,
    /// Project frame index last rendered while playing, for pacing.
    last_played_frame: i64,
    clips_model: Rc<VecModel<TimelineClip>>,
    /// Text lane and preview overlay rows. Updated in place: replacing the
    /// model would rebuild the elements, including the one being dragged.
    text_blocks_model: Rc<VecModel<crate::ui::TextBlockView>>,
    preview_texts_model: Rc<VecModel<crate::ui::PreviewText>>,
    strip_thumbs: HashMap<MediaId, slint::Image>,
    preview_dirty: bool,
    last_preview: Instant,
    dirty_since: Option<Instant>,
    last_prune: Instant,
    /// When the status message was set; cleared after `STATUS_VISIBLE`.
    status_since: Option<Instant>,
    drag: Option<(usize, bool)>,
    /// Trim gesture in progress: shown live, committed on release.
    trim: Option<TrimState>,
    export: Option<ExportRun>,
    /// The last finished export, for "Show in Finder".
    last_export: Option<PathBuf>,
    /// The music lane is selected (the inspector shows the music).
    music_selected: bool,
    pending_songs: music_titles::PendingSongs,
    /// Selected texts (exclusive with clips and music).
    text_selection: Vec<clipforge_core::TextId>,
    /// A text drag on the preview or the lane in progress.
    text_drag: Option<texts::TextDrag>,
    /// The text being edited in place on the preview.
    editing_text: Option<clipforge_core::TextId>,
    /// Keyboard focus on the text lane.
    text_focus: Option<clipforge_core::TextId>,
    /// Lays texts out for hit boxes on the preview; created on first use
    /// (the font catalogue loads in the background at start-up).
    text_measure: std::cell::OnceCell<clipforge_render::text::TextRenderer>,
}

impl EditorController {
    pub(crate) fn new(
        window: &MainWindow,
        library: Arc<Library>,
        scheduler: Arc<Scheduler>,
        backends: Backends,
        dirs: AppDirs,
    ) -> Self {
        let clips_model = Rc::new(VecModel::default());
        let state = window.global::<EditorState>();
        state.set_clips(ModelRc::from(Rc::clone(&clips_model)));
        let text_blocks_model = Rc::new(VecModel::default());
        state.set_text_blocks(ModelRc::from(Rc::clone(&text_blocks_model)));
        let preview_texts_model = Rc::new(VecModel::default());
        state.set_preview_texts(ModelRc::from(Rc::clone(&preview_texts_model)));

        let mut project = Project::new();
        let autosave = dirs.data.join("autosave.clipforge.json");
        if let Ok(text) = std::fs::read_to_string(&autosave)
            && let Ok(p) = Project::from_json(&text)
        {
            info!(clips = p.clips.len(), "recovered autosaved project");
            project = p;
        }

        let preview: Arc<PreviewSources> = Arc::new(PreviewSources {
            images: Mutex::new(HashMap::new()),
            missing: Mutex::new(HashSet::new()),
            player: Arc::new(Player::new(
                FfmpegCli::discover(),
                clipforge_render::PREVIEW_LONG_EDGE,
            )),
            videos: Mutex::new(HashMap::new()),
            backends: backends.clone(),
        });
        let inner = Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            library,
            scheduler,
            backends: backends.clone(),
            dirs,
            project,
            history: History::new(),
            selection: Selection::default(),
            project_path: None,
            playhead: Ticks::ZERO,
            playing: false,
            last_tick: Instant::now(),
            pps: editor_view::DEFAULT_PIXELS_PER_SECOND,
            preview: Arc::clone(&preview),
            worker: PreviewWorker::start(preview),
            snapshot: Arc::new(Project::new()),
            last_played_frame: -1,
            clips_model,
            text_blocks_model,
            preview_texts_model,
            strip_thumbs: HashMap::new(),
            preview_dirty: true,
            last_preview: Instant::now() - PREVIEW_MIN_INTERVAL,
            dirty_since: None,
            last_prune: Instant::now(),
            status_since: None,
            drag: None,
            trim: None,
            export: None,
            last_export: None,
            music_selected: false,
            pending_songs: music_titles::PendingSongs::default(),
            text_selection: Vec::new(),
            text_drag: None,
            editing_text: None,
            text_focus: None,
            text_measure: std::cell::OnceCell::new(),
        }));

        macro_rules! on {
            ($setter:ident, |$i:ident $(, $arg:ident)*| $body:expr) => {{
                let inner = Rc::clone(&inner);
                state.$setter(move |$($arg),*| {
                    #[allow(unused_mut)]
                    let mut $i = inner.borrow_mut();
                    $body
                });
            }};
        }
        on!(on_clip_pressed, |i, idx, shift, toggle| i
            .clip_pressed(idx, shift, toggle));
        on!(on_clip_dragged, |i, x| i.clip_dragged(x));
        on!(on_clip_released, |i, x| i.clip_released(x));
        on!(on_scrub, |i, x| i.scrub(x));
        on!(on_toggle_play, |i| i.toggle_play());
        on!(on_zoom_changed, |i, pps| {
            i.pps = pps;
            i.sync_timeline();
        });
        on!(on_select_all, |i| i.select_all());
        on!(on_clear_selection, |i| i.clear_selection());
        on!(on_clip_navigate, |i, forward, mode| i
            .clip_navigate(forward, mode));
        on!(on_toggle_focused_clip, |i| i.toggle_focused_clip());
        on!(on_nudge_clips, |i, later| i.nudge_clips(later));
        on!(on_strip_focus_entered, |i| i.strip_focus_entered());
        on!(on_delete_selected, |i| i.delete_selected());
        on!(on_undo, |i| i.undo());
        on!(on_redo, |i| i.redo());
        on!(on_duration_changed, |i, secs| i.set_duration(secs));
        on!(on_fit_changed, |i, idx| i.set_fit(idx));
        on!(on_transition_changed, |i, idx, secs| i
            .set_transition(idx, secs));
        on!(on_rotate_selected, |i| i.rotate_selected());
        on!(on_shuffle_transitions, |i| i.shuffle_transitions());
        on!(on_motion_changed, |i, idx| i.set_motion(idx));
        on!(on_shuffle_motion, |i| i.shuffle_motion());
        on!(on_aspect_changed, |i, idx| i.set_aspect(idx));
        on!(on_muted_changed, |i, muted| i.set_muted(muted));
        on!(on_volume_changed, |i, percent| i.set_volume(percent));
        on!(on_trim_dragged, |i, idx, left, x| i
            .trim_dragged(idx, left, x));
        on!(on_trim_released, |i, idx, left, x| i
            .trim_released(idx, left, x));
        on!(on_step_frames, |i, n| i.step_frames(n));
        on!(on_step_seconds, |i, secs| i.step_seconds(secs));
        on!(on_pause, |i| i.set_playing(false));
        on!(on_play, |i| i.set_playing(true));
        on!(on_new_project, |i| i.new_project());
        on!(on_open_project, |i| i.open_project());
        on!(on_save_project, |i| {
            i.save_project(false);
        });
        on!(on_save_project_as, |i| {
            i.save_project(true);
        });
        on!(on_export_start, |i| i.export_start());
        on!(on_export_cancel, |i| i.export_cancel());
        on!(on_export_refresh, |i| i.export_refresh());
        on!(on_export_reveal, |i| i.export_reveal());
        on!(on_add_text, |i| i.add_text());
        on!(on_text_lane_focus_entered, |i| i.text_lane_focus_entered());
        on!(on_text_lane_navigate, |i, forward| i
            .text_lane_navigate(forward));
        on!(on_text_lane_activate, |i| i.text_lane_activate());
        on!(on_text_lane_pressed, |i, idx, shift, toggle, code, x| i
            .text_lane_pressed(idx, shift, toggle, code, x));
        on!(on_text_lane_dragged, |i, x| i.text_lane_dragged(x));
        on!(on_text_released, |i| i.text_released());
        on!(on_text_lane_double_clicked, |i, x| i
            .text_lane_double_clicked(x));
        on!(on_preview_text_pressed, |i, idx, code, nx, ny| i
            .preview_text_pressed(idx, code, nx, ny));
        on!(on_preview_text_dragged, |i, nx, ny| i
            .preview_text_dragged(nx, ny));
        on!(on_preview_background_pressed, |i| i
            .preview_background_pressed());
        on!(on_preview_text_double_clicked, |i, idx| i
            .begin_inline_edit(idx));
        on!(on_edit_selected_text, |i| i.edit_selected_text());
        on!(on_inline_edit_finished, |i| i.end_inline_edit());
        on!(on_text_typed, |i, text| i.text_typed(&text));
        on!(on_text_typing_done, |i| i.text_typing_done());
        on!(on_text_nudge, |i, dx, dy, large| i
            .text_nudge(dx, dy, large));
        on!(on_text_font_changed, |i, name| i.text_font(&name));
        on!(on_text_font_search, |i, query| i.text_font_search(&query));
        on!(on_text_points_changed, |i, pt| i.text_points(pt));
        on!(on_text_underline_changed, |i, on| i.text_underline(on));
        on!(on_text_bold_changed, |i, on| i.text_bold(on));
        on!(on_text_italic_changed, |i, on| i.text_italic(on));
        on!(on_text_align_changed, |i, idx| i.text_align(idx));
        on!(on_text_color_changed, |i, idx| i.text_color(idx));
        on!(on_text_box_changed, |i, on| i.text_box(on));
        on!(on_text_box_color_changed, |i, idx| i.text_box_color(idx));
        on!(on_text_shadow_changed, |i, on| i.text_shadow(on));
        on!(on_text_duration_changed, |i, v| i.text_duration(v));
        on!(on_text_enter_changed, |i, idx, secs| i
            .text_enter(idx, secs));
        on!(on_text_exit_changed, |i, idx, secs| i.text_exit(idx, secs));
        on!(on_title_background_changed, |i, idx| i
            .title_background_changed(idx));
        on!(on_add_opening_title, |i| i.add_title(false));
        on!(on_add_closing_card, |i| i.add_title(true));
        on!(on_music_lane_pressed, |i| i.music_lane_pressed());
        on!(on_add_music, |i| i.add_music());
        on!(on_remove_all_songs, |i| i.remove_all_songs());
        on!(on_song_move, |i, idx, up| i.song_move(idx, up));
        on!(on_song_remove, |i, idx| i.song_remove(idx));
        on!(on_music_volume_changed, |i, v| i.music_volume(v));
        on!(on_music_fades_changed, |i, fade_in, fade_out| i
            .music_fades(fade_in, fade_out));
        on!(on_music_loop_changed, |i, on| i.music_loop(on));
        on!(on_music_duck_changed, |i, on| i.music_duck(on));
        on!(on_fit_to_music, |i| i.fit_to_music());

        let timer = Timer::default();
        {
            let inner = Rc::clone(&inner);
            timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
                let mut i = inner.borrow_mut();
                i.tick();
            });
        }
        inner.borrow_mut().sync_all();
        EditorController {
            inner,
            _timer: timer,
        }
    }

    /// Closure handles for other controllers.
    pub(crate) fn add_media_handle(&self) -> Rc<dyn Fn(Vec<MediaId>)> {
        let inner = Rc::clone(&self.inner);
        Rc::new(move |ids| inner.borrow_mut().add_media(&ids))
    }

    /// Drag hover from the library: `Some(x)` in strip content pixels shows
    /// the drop marker, `None` hides it.
    pub(crate) fn drop_hover_handle(&self) -> Rc<dyn Fn(Option<f32>)> {
        let inner = Rc::clone(&self.inner);
        Rc::new(move |x| inner.borrow_mut().library_drop_hover(x))
    }

    /// Drop from the library at strip content x.
    pub(crate) fn drop_insert_handle(&self) -> Rc<dyn Fn(Vec<MediaId>, f32)> {
        let inner = Rc::clone(&self.inner);
        Rc::new(move |ids, x| inner.borrow_mut().insert_media_at_x(&ids, x))
    }

    pub(crate) fn preview_thumb_handle(&self) -> Rc<dyn Fn(MediaId, PathBuf)> {
        let inner = Rc::clone(&self.inner);
        Rc::new(move |id, path| inner.borrow_mut().load_preview_image(id, &path))
    }

    /// Saves the autosave file now (app exit).
    pub(crate) fn flush_autosave(&self) {
        self.inner.borrow_mut().autosave();
    }
}

impl Inner {
    fn state(&self) -> Option<MainWindow> {
        self.window.upgrade()
    }

    // ----- commands -------------------------------------------------------

    fn apply(&mut self, command: Command) {
        match self.history.apply(&mut self.project, command) {
            Ok(()) => {
                self.selection.retain_existing(&self.project.clips);
                self.retain_text_selection();
                self.dirty_since.get_or_insert_with(Instant::now);
                self.sync_all();
            }
            Err(e) => warn!(error = %e, "command rejected"),
        }
    }

    /// Indices a bulk edit applies to: the selection, or every clip.
    fn targets(&self) -> Vec<usize> {
        if self.selection.is_empty() {
            (0..self.project.clips.len()).collect()
        } else {
            self.selection.indices(&self.project.clips)
        }
    }

    fn add_media(&mut self, ids: &[MediaId]) {
        let at = self.project.clips.len();
        self.insert_media(ids, at);
    }

    fn insert_media_at_x(&mut self, ids: &[MediaId], x: f32) {
        let boxes = layout(&self.project.clips, self.pps);
        let at = editor_view::drop_index(&boxes, x);
        self.insert_media(ids, at);
        self.library_drop_hover(None);
    }

    fn library_drop_hover(&mut self, x: Option<f32>) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        match x {
            Some(x) => {
                let boxes = layout(&self.project.clips, self.pps);
                let to = editor_view::drop_index(&boxes, x);
                let marker_x = if to < boxes.len() {
                    boxes[to].x
                } else {
                    editor_view::strip_width(&boxes)
                };
                s.set_drop_marker(i32::try_from(to).unwrap_or(-1));
                s.set_drop_marker_x(marker_x);
            }
            None => s.set_drop_marker(-1),
        }
    }

    fn insert_media(&mut self, ids: &[MediaId], at: usize) {
        let at = at.min(self.project.clips.len());
        let mut refs = Vec::new();
        let mut skipped = 0usize;
        {
            let cat = self.library.catalogue();
            for id in ids {
                match cat.get(*id).ok().as_ref().and_then(media_ref_for) {
                    Some(r) => refs.push(r),
                    None => skipped += 1,
                }
            }
        }
        if refs.is_empty() {
            self.set_status(if skipped > 0 { 2 } else { 0 }, skipped);
            return;
        }
        let added = refs.len();
        // Audio goes to the music; photos and videos become clips. One undo step.
        let (refs, songs) = editor_view::split_songs(refs);
        let mut commands = Vec::new();
        if !refs.is_empty() {
            let clips = clips_for(&self.project, &refs);
            let entries = clips
                .into_iter()
                .enumerate()
                .map(|(k, c)| (at + k, c))
                .collect();
            commands.push(Command::InsertClips {
                entries,
                media: refs,
            });
        }
        commands.extend(self.songs_command(songs));
        self.apply(Command::Batch { commands });
        self.set_status(
            if skipped > 0 { 2 } else { 1 },
            if skipped > 0 { skipped } else { added },
        );
        if self.playhead >= total_duration(&self.project.clips) {
            self.playhead = Ticks::ZERO;
        }
        self.preview_dirty = true;
    }

    /// Status kinds: 0 none, 1 added N, 2 skipped N, 3 shuffled transitions
    /// on N clips, 4 shuffled motion on N photos.
    fn set_status(&mut self, kind: u8, n: usize) {
        self.status_since = (kind != 0).then(Instant::now);
        if let Some(w) = self.state() {
            let strings = w.global::<crate::ui::Strings>();
            strings.set_count(i32::try_from(n).unwrap_or(i32::MAX));
            let text = match kind {
                1 => strings.get_added_clips(),
                2 => strings.get_skipped_items(),
                3 => strings.get_shuffled_transitions(),
                4 => strings.get_shuffled_motion(),
                _ => SharedString::default(),
            };
            w.global::<EditorState>().set_status_text(text);
        }
    }

    fn select_all(&mut self) {
        self.leave_music();
        self.leave_texts();
        self.selection.select_all(&self.project.clips);
        self.sync_timeline();
    }

    /// Escape: drop the selection (DESIGN.md §3 selection model).
    fn clear_selection(&mut self) {
        // Escape first cancels a gesture in progress (DESIGN.md §5).
        if self.cancel_trim() || self.cancel_text_drag() {
            return;
        }
        if self.editing_text.is_some() {
            self.end_inline_edit();
            self.sync_all();
            return;
        }
        self.leave_music();
        self.leave_texts();
        self.selection.clear();
        self.preview_dirty = true;
        self.sync_all();
    }

    // ----- keyboard navigation (docs/ux/decisions/keyboard-navigation.md) --

    /// Where focus goes when the strip is entered: the focused clip, else
    /// the first selected one, else the clip under the playhead.
    fn entry_clip(&self) -> Option<usize> {
        let clips = &self.project.clips;
        if clips.is_empty() {
            return None;
        }
        self.selection
            .focus_index(clips)
            .or_else(|| self.selection.indices(clips).first().copied())
            .or_else(|| {
                clipforge_core::timeline::placements(clips)
                    .iter()
                    .rposition(|p| p.start <= self.playhead)
            })
            .or(Some(0))
    }

    /// ↑/↓: previous/next clip. The playhead jumps to its start (edit point).
    fn clip_navigate(&mut self, forward: bool, mode: i32) {
        let len = self.project.clips.len();
        let target = match self.selection.focus_index(&self.project.clips) {
            Some(from) => clip_nav(Some(from), len, forward),
            // The first key press only lands on the entry clip.
            None => self.entry_clip(),
        };
        let Some(target) = target else { return };
        match mode {
            1 => self
                .selection
                .click(&self.project.clips, target, true, false),
            2 => self.selection.set_focus(&self.project.clips, target),
            _ => self
                .selection
                .click(&self.project.clips, target, false, false),
        }
        if let Some(p) = clipforge_core::timeline::placements(&self.project.clips).get(target) {
            self.playhead = p.start;
            self.preview_dirty = true;
        }
        self.sync_timeline();
        self.reveal_focused_clip();
    }

    /// Enter: flip the focused clip in or out of the selection.
    fn toggle_focused_clip(&mut self) {
        if self.selection.focus_index(&self.project.clips).is_none()
            && let Some(i) = self.entry_clip()
        {
            self.selection.set_focus(&self.project.clips, i);
        }
        self.selection.toggle_focused(&self.project.clips);
        self.sync_timeline();
        self.reveal_focused_clip();
    }

    /// Alt/Option+←/→: move the selection (or the focused clip) one place.
    fn nudge_clips(&mut self, later: bool) {
        let clips = &self.project.clips;
        let mut indices = self.selection.indices(clips);
        if indices.is_empty()
            && let Some(f) = self.selection.focus_index(clips)
        {
            indices.push(f);
        }
        let dir = if later { Nudge::Later } else { Nudge::Earlier };
        let Some(to) = nudge_target(clips.len(), &indices, dir) else {
            return;
        };
        if self.selection.focus_index(clips).is_none()
            && let Some(first) = indices.first()
        {
            self.selection.set_focus(clips, *first);
        }
        if let Some(cmd) = Command::move_clips(clips.len(), &indices, to) {
            self.apply(cmd);
            self.preview_dirty = true;
            self.reveal_focused_clip();
        }
    }

    fn strip_focus_entered(&mut self) {
        if self.selection.focus_index(&self.project.clips).is_none()
            && let Some(i) = self.entry_clip()
        {
            self.selection.set_focus(&self.project.clips, i);
        }
        self.sync_timeline();
        self.reveal_focused_clip();
    }

    /// Publishes the focused clip's box so the strip scrolls it into view.
    fn reveal_focused_clip(&self) {
        let Some(i) = self.selection.focus_index(&self.project.clips) else {
            return;
        };
        let boxes = layout(&self.project.clips, self.pps);
        let (Some(b), Some(w)) = (boxes.get(i), self.state()) else {
            return;
        };
        let s = w.global::<EditorState>();
        s.set_focus_clip_x(b.x);
        s.set_focus_clip_width(b.width);
        s.set_focus_token(s.get_focus_token().wrapping_add(1));
    }

    fn delete_selected(&mut self) {
        if self.music_selected {
            self.remove_all_songs();
            return;
        }
        if self.remove_selected_texts() {
            return;
        }
        let indices = self.selection.indices(&self.project.clips);
        if indices.is_empty() {
            return;
        }
        self.selection.clear();
        self.apply(Command::RemoveClips { indices });
        self.preview_dirty = true;
    }

    fn undo(&mut self) {
        if self.history.undo(&mut self.project).is_some() {
            self.selection.retain_existing(&self.project.clips);
            self.retain_text_selection();
            self.dirty_since.get_or_insert_with(Instant::now);
            self.preview_dirty = true;
            self.sync_all();
        }
    }

    fn redo(&mut self) {
        if self.history.redo(&mut self.project).is_some() {
            self.selection.retain_existing(&self.project.clips);
            self.retain_text_selection();
            self.dirty_since.get_or_insert_with(Instant::now);
            self.preview_dirty = true;
            self.sync_all();
        }
    }

    fn set_duration(&mut self, secs: f32) {
        let indices: Vec<usize> = self
            .targets()
            .into_iter()
            .filter(|&i| self.project.clips[i].is_still())
            .collect();
        if indices.is_empty() {
            return;
        }
        let duration = Ticks::from_seconds_f64(f64::from(secs.max(0.1)));
        // Also remember as the default for newly added photos when applied to all.
        let mut commands = vec![Command::SetPhotoDuration { indices, duration }];
        if self.selection.is_empty() {
            let settings = ProjectSettings {
                default_photo_duration: duration,
                ..self.project.settings.clone()
            };
            commands.push(Command::SetSettings { settings });
        }
        self.apply(Command::Batch { commands });
        self.preview_dirty = true;
    }

    fn set_fit(&mut self, idx: i32) {
        let fit = if idx == 1 { Fit::Cover } else { Fit::Contain };
        let indices = self.targets();
        if indices.is_empty() {
            return;
        }
        let mut commands = vec![Command::SetFit { indices, fit }];
        if self.selection.is_empty() {
            commands.push(Command::SetSettings {
                settings: ProjectSettings {
                    default_fit: fit,
                    ..self.project.settings.clone()
                },
            });
        }
        self.apply(Command::Batch { commands });
        self.preview_dirty = true;
    }

    fn set_transition(&mut self, idx: i32, secs: f32) {
        let kind = TransitionKind::from_index(usize::try_from(idx).unwrap_or(0));
        let transition = Transition {
            kind,
            duration: Ticks::from_seconds_f64(f64::from(secs.max(0.1))),
        };
        let indices = self.targets();
        if indices.is_empty() {
            return;
        }
        let mut commands = vec![Command::SetTransition {
            indices,
            transition,
        }];
        if self.selection.is_empty() {
            commands.push(Command::SetSettings {
                settings: ProjectSettings {
                    default_transition: transition,
                    ..self.project.settings.clone()
                },
            });
        }
        self.apply(Command::Batch { commands });
        self.preview_dirty = true;
    }

    /// Varied transitions on the targets, keeping the current duration.
    /// One undo step; the defaults for new clips are not touched.
    fn shuffle_transitions(&mut self) {
        let indices = self.targets();
        if indices.is_empty() {
            return;
        }
        let secs = self
            .state()
            .map_or(1.0, |w| w.global::<EditorState>().get_transition_seconds());
        let duration = Ticks::from_seconds_f64(f64::from(secs.max(0.1)));
        let entries = clipforge_core::shuffle::transitions(&indices, duration, shuffle_seed());
        let n = entries.len();
        self.apply(Command::SetTransitionEach { entries });
        self.set_status(3, n);
        self.preview_dirty = true;
    }

    fn set_motion(&mut self, idx: i32) {
        let motion = clipforge_core::Motion::from_index(usize::try_from(idx).unwrap_or(0));
        let targets = self.targets();
        let indices = editor_view::photo_targets(&self.project, &targets);
        let mut commands = Vec::new();
        if !indices.is_empty() {
            commands.push(Command::SetMotion { indices, motion });
        }
        if self.selection.is_empty() {
            commands.push(Command::SetSettings {
                settings: ProjectSettings {
                    default_motion: motion,
                    ..self.project.settings.clone()
                },
            });
        }
        if commands.is_empty() {
            return;
        }
        self.apply(Command::Batch { commands });
        self.preview_dirty = true;
    }

    /// Varied movement on the photo targets. One undo step.
    fn shuffle_motion(&mut self) {
        let targets = self.targets();
        let indices = editor_view::photo_targets(&self.project, &targets);
        if indices.is_empty() {
            return;
        }
        let entries = clipforge_core::shuffle::motions(&indices, shuffle_seed());
        let n = entries.len();
        self.apply(Command::SetMotionEach { entries });
        self.set_status(4, n);
        self.preview_dirty = true;
    }

    fn rotate_selected(&mut self) {
        let indices = self.targets();
        if indices.is_empty() {
            return;
        }
        // Rotate each clip one step further than it is now.
        let commands: Vec<Command> = indices
            .iter()
            .map(|&i| Command::SetRotate {
                indices: vec![i],
                rotate: self.project.clips[i].rotate.next(),
            })
            .collect();
        self.apply(Command::Batch { commands });
        self.preview_dirty = true;
    }

    fn set_aspect(&mut self, idx: i32) {
        let aspect = if idx == 1 {
            Aspect::Portrait9x16
        } else {
            Aspect::Landscape16x9
        };
        if aspect == self.project.settings.aspect {
            return;
        }
        self.apply(Command::SetSettings {
            settings: ProjectSettings {
                aspect,
                ..self.project.settings.clone()
            },
        });
        self.preview_dirty = true;
    }

    fn set_muted(&mut self, muted: bool) {
        let indices: Vec<usize> = self
            .targets()
            .into_iter()
            .filter(|&i| !self.project.clips[i].is_photo())
            .collect();
        if indices.is_empty() {
            return;
        }
        self.apply(Command::SetMuted { indices, muted });
    }

    fn set_volume(&mut self, percent: f32) {
        let indices: Vec<usize> = self
            .targets()
            .into_iter()
            .filter(|&i| !self.project.clips[i].is_photo())
            .collect();
        if indices.is_empty() {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let percent = percent.round().clamp(0.0, 300.0) as u16;
        self.apply(Command::SetVolume { indices, percent });
    }

    /// Clips as currently displayed: the project, with a trim in progress
    /// applied. Display only; the project changes on release.
    fn display_clips(&self) -> std::borrow::Cow<'_, [clipforge_core::Clip]> {
        match self.trim {
            Some(t) if !t.cancelled => {
                let mut clips = self.project.clips.clone();
                if let Some(c) = clips.get_mut(t.anchor.index) {
                    c.source = clipforge_core::ClipSource::Video {
                        in_point: t.in_point,
                        out_point: t.out_point,
                    };
                }
                std::borrow::Cow::Owned(clips)
            }
            _ => std::borrow::Cow::Borrowed(&self.project.clips),
        }
    }

    /// Clip boxes as displayed. A right-edge trim ripples live (later clips
    /// move with the edge); a left-edge trim keeps everything in place and
    /// moves only the trimmed clip's left edge, so it sits under the cursor.
    fn display_boxes(&self) -> Vec<editor_view::ClipBox> {
        match self.trim {
            Some(t) if !t.cancelled && t.anchor.left => {
                let mut boxes = layout(&self.project.clips, self.pps);
                if let Some(b) = boxes.get_mut(t.anchor.index) {
                    *b = editor_view::left_trim_box(&t.anchor, t.in_point, self.pps);
                }
                boxes
            }
            _ => layout(&self.display_clips(), self.pps),
        }
    }

    /// Starts a trim gesture on first move: remembers the clip's trim and
    /// its on-screen edges.
    fn begin_trim(&mut self, index: usize, left: bool) -> Option<TrimState> {
        let clip = self.project.clips.get(index)?;
        let clipforge_core::ClipSource::Video {
            in_point,
            out_point,
        } = clip.source
        else {
            return None;
        };
        let natural = self
            .project
            .media
            .get(&clip.media)
            .and_then(|m| m.duration)?;
        let boxes = layout(&self.project.clips, self.pps);
        let b = boxes.get(index)?;
        let anchor = editor_view::TrimAnchor {
            index,
            left,
            in_point,
            out_point,
            natural,
            start_x: b.x,
            end_x: b.x + b.width,
        };
        Some(TrimState {
            anchor,
            in_point,
            out_point,
            cancelled: false,
        })
    }

    fn trim_dragged(&mut self, idx: i32, left: bool, x: f32) {
        let Ok(index) = usize::try_from(idx) else {
            return;
        };
        let mut state = match self.trim {
            Some(t) if t.cancelled => return,
            Some(t) if t.anchor.index == index && t.anchor.left == left => t,
            _ => match self.begin_trim(index, left) {
                Some(t) => t,
                None => return,
            },
        };
        let (in_point, out_point) = editor_view::trim_at_cursor(&state.anchor, x, self.pps);
        state.in_point = in_point;
        state.out_point = out_point;
        self.trim = Some(state);
        // Preview the new first / last frame from the displayed clips.
        let mut shown = self.project.clone();
        shown.clips = self.display_clips().into_owned();
        let places = clipforge_core::timeline::placements(&shown.clips);
        if let Some(p) = places.get(index) {
            self.playhead = if left {
                p.start
            } else {
                p.end - Ticks::from_flicks(1)
            };
        }
        self.snapshot = Arc::new(shown);
        self.set_playing(false);
        self.preview_dirty = true;
        self.sync_timeline();
    }

    fn trim_released(&mut self, idx: i32, left: bool, x: f32) {
        let Some(state) = self.trim.take() else {
            return;
        };
        let Ok(index) = usize::try_from(idx) else {
            return;
        };
        if state.cancelled || state.anchor.index != index || state.anchor.left != left {
            self.sync_all();
            return;
        }
        let (in_point, out_point) = editor_view::trim_at_cursor(&state.anchor, x, self.pps);
        if (in_point, out_point) == (state.anchor.in_point, state.anchor.out_point) {
            self.sync_all();
            return;
        }
        self.apply(Command::SetTrim {
            index,
            in_point,
            out_point,
        });
        self.preview_dirty = true;
    }

    /// Escape during a trim: back to the original trim, no undo step.
    fn cancel_trim(&mut self) -> bool {
        match self.trim.as_mut() {
            Some(t) if !t.cancelled => {
                t.cancelled = true;
                self.sync_all();
                self.preview_dirty = true;
                true
            }
            _ => false,
        }
    }

    fn step_frames(&mut self, n: i32) {
        self.set_playing(false);
        let fd = self.project.settings.frame_rate.frame_duration();
        let total = total_duration(&self.project.clips);
        self.playhead = (self.playhead + fd * i64::from(n)).clamp(Ticks::ZERO, total);
        self.preview_dirty = true;
        self.sync_transport();
    }

    fn step_seconds(&mut self, secs: f32) {
        self.set_playing(false);
        let total = total_duration(&self.project.clips);
        self.playhead =
            (self.playhead + Ticks::from_seconds_f64(f64::from(secs))).clamp(Ticks::ZERO, total);
        self.preview_dirty = true;
        self.sync_transport();
    }

    fn set_playing(&mut self, playing: bool) {
        if playing == self.playing {
            return;
        }
        if playing {
            if self.project.clips.is_empty() {
                return;
            }
            if self.playhead >= total_duration(&self.project.clips) {
                self.playhead = Ticks::ZERO;
            }
            self.playing = true;
            self.last_tick = Instant::now();
            self.preview.player.play_audio(&self.project, self.playhead);
        } else {
            self.playing = false;
            self.preview.player.stop_audio();
        }
        self.sync_transport();
    }

    // ----- selection & drag -----------------------------------------------

    fn clip_pressed(&mut self, idx: i32, shift: bool, toggle: bool) {
        let Ok(index) = usize::try_from(idx) else {
            return;
        };
        self.leave_music();
        self.leave_texts();
        let already_selected = self
            .project
            .clips
            .get(index)
            .is_some_and(|c| self.selection.ids.contains(&c.id));
        if already_selected && !shift && !toggle {
            // Keep the group for dragging; the keyboard continues from here.
            self.selection.set_focus(&self.project.clips, index);
        } else {
            self.selection
                .click(&self.project.clips, index, shift, toggle);
        }
        self.drag = Some((index, already_selected && !shift && !toggle));
        // Move the playhead to the clip start for immediate feedback.
        if let Some(p) = clipforge_core::timeline::placements(&self.project.clips).get(index) {
            self.playhead = p.start;
            self.preview_dirty = true;
        }
        self.sync_timeline();
    }

    fn clip_dragged(&mut self, x: f32) {
        if self.drag.is_none() {
            return;
        }
        let boxes = layout(&self.project.clips, self.pps);
        let to = editor_view::drop_index(&boxes, x);
        let marker_x = if to < boxes.len() {
            boxes[to].x
        } else {
            editor_view::strip_width(&boxes)
        };
        if let Some(w) = self.state() {
            let s = w.global::<EditorState>();
            s.set_drop_marker(i32::try_from(to).unwrap_or(-1));
            s.set_drop_marker_x(marker_x);
        }
    }

    fn clip_released(&mut self, x: f32) {
        let Some((pressed_index, was_selected)) = self.drag.take() else {
            return;
        };
        let had_marker = self
            .state()
            .is_some_and(|w| w.global::<EditorState>().get_drop_marker() >= 0);
        if let Some(w) = self.state() {
            w.global::<EditorState>().set_drop_marker(-1);
        }
        if !had_marker {
            // A plain click on an already selected clip (no drag) makes it the only selection.
            if was_selected {
                self.selection
                    .click(&self.project.clips, pressed_index, false, false);
                self.sync_timeline();
            }
            return;
        }
        let boxes = layout(&self.project.clips, self.pps);
        let to = editor_view::drop_index(&boxes, x);
        let moving = if self
            .selection
            .ids
            .contains(&self.project.clips[pressed_index].id)
        {
            self.selection.indices(&self.project.clips)
        } else {
            vec![pressed_index]
        };
        if let Some(cmd) = Command::move_clips(self.project.clips.len(), &moving, to) {
            self.apply(cmd);
            self.preview_dirty = true;
        } else {
            self.sync_timeline();
        }
    }

    // ----- transport --------------------------------------------------------

    fn scrub(&mut self, x: f32) {
        if self.music_selected || !self.text_selection.is_empty() {
            // A click on empty strip space leaves the music and the texts.
            self.leave_music();
            self.leave_texts();
            self.sync_all();
        }
        let boxes = layout(&self.project.clips, self.pps);
        self.playhead = editor_view::time_at_x(&self.project.clips, &boxes, x);
        self.set_playing(false);
        self.preview_dirty = true;
        self.sync_transport();
    }

    fn toggle_play(&mut self) {
        let next = !self.playing;
        self.set_playing(next);
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.playing {
            let elapsed = now - self.last_tick;
            self.last_tick = now;
            self.playhead += Ticks::from_seconds_f64(elapsed.as_secs_f64());
            let total = total_duration(&self.project.clips);
            if self.playhead >= total {
                self.playhead = total;
                self.set_playing(false);
            }
            // Re-render only when the playhead reaches a new project frame;
            // rendering at 60 Hz for a 30 fps timeline doubles the work.
            let frame_index = self.playhead.to_frames(self.project.settings.frame_rate);
            if frame_index != self.last_played_frame {
                self.last_played_frame = frame_index;
                self.preview_dirty = true;
            }
            self.sync_transport();
        }
        if self.preview.player.take_changed() {
            self.preview_dirty = true;
        }
        if self
            .status_since
            .is_some_and(|t| now.duration_since(t) >= STATUS_VISIBLE)
        {
            self.status_since = None;
            if let Some(w) = self.state() {
                w.global::<EditorState>()
                    .set_status_text(SharedString::default());
            }
        }
        if let Some(frame) = self.worker.take_frame()
            && let Some(w) = self.state()
        {
            w.global::<EditorState>()
                .set_preview(frame_to_image(&frame));
        }
        if now.duration_since(self.last_prune) > Duration::from_secs(2) {
            self.last_prune = now;
            self.preview.player.prune();
        }
        if self.preview_dirty && now - self.last_preview >= PREVIEW_MIN_INTERVAL {
            self.render_preview();
        }
        if let Some(since) = self.dirty_since
            && now - since >= AUTOSAVE_DELAY
        {
            self.autosave();
        }
        self.poll_export();
        self.poll_pending_songs(false);
    }

    // ----- preview ----------------------------------------------------------

    /// Hands the current playhead to the worker thread. The finished frame
    /// is picked up in `tick`.
    fn render_preview(&mut self) {
        self.preview_dirty = false;
        self.last_preview = Instant::now();
        // Stills the previous render lacked: fetch their preview thumbnails.
        for id in self.preview.take_missing() {
            if let Some(path) =
                self.library
                    .request_thumb(id, ThumbLevel::Preview, Priority::Interactive)
            {
                self.load_preview_image(id, &path);
            }
        }
        self.worker
            .request(Arc::clone(&self.snapshot), self.playhead);
    }

    fn load_preview_image(&mut self, id: MediaId, path: &std::path::Path) {
        let Ok(img) = image::open(path) else { return };
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        lock(&self.preview.images).insert(
            id,
            SourceImage {
                width: w,
                height: h,
                rgba: Arc::new(rgba.into_raw()),
            },
        );
        if self.project.clips.iter().any(|c| c.media == id) {
            self.preview_dirty = true;
        }
    }

    // ----- sync to Slint ----------------------------------------------------

    fn sync_all(&mut self) {
        self.snapshot = Arc::new(self.preview_project());
        for m in self.project.media.values() {
            if m.kind == clipforge_core::RefKind::Video {
                self.preview.register_video(m.id, &m.path);
            }
        }
        self.sync_timeline();
        self.sync_transport();
        self.sync_inspector();
        self.sync_project();
    }

    fn sync_timeline(&mut self) {
        let boxes = self.display_boxes();
        let clips = self.display_clips().into_owned();
        let thumbs: Vec<Option<slint::Image>> = clips
            .iter()
            .map(|c| (!c.is_title()).then(|| self.strip_thumb(c.media)).flatten())
            .collect();
        let mut rows = Vec::with_capacity(clips.len());
        for (i, ((clip, b), thumb)) in clips.iter().zip(&boxes).zip(thumbs).enumerate() {
            rows.push(TimelineClip {
                index: i32::try_from(i).unwrap_or(0),
                id: SharedString::from(clip.id.to_string()),
                title: SharedString::from(
                    self.project
                        .media
                        .get(&clip.media)
                        .map(|m| m.name.clone())
                        .unwrap_or_default(),
                ),
                has_thumb: thumb.is_some(),
                thumb: thumb.unwrap_or_default(),
                x: b.x,
                width: b.width,
                overlap: b.overlap,
                selected: self.selection.ids.contains(&clip.id),
                duration_text: SharedString::from(format::duration(clip.duration())),
                transition: editor_view::clip_flags(clip).transition,
                is_video: clip.is_video(),
                muted: clip.muted,
                moving: editor_view::clip_flags(clip).moving,
                focused: self.selection.focus == Some(clip.id),
                is_title: clip.is_title(),
                title_bg: match clip.source {
                    clipforge_core::ClipSource::Title { background, .. } => {
                        let [r, g, b] = clipforge_render::title_rgb(background);
                        slint::Color::from_rgb_u8(r, g, b)
                    }
                    _ => slint::Color::default(),
                },
            });
        }
        // Reuse the model in place to avoid flicker.
        update_rows(&self.clips_model, rows);
        if let Some(w) = self.state() {
            let s = w.global::<EditorState>();
            s.set_strip_width(editor_view::strip_width(&boxes));
            s.set_selected_count(i32::try_from(self.selection.ids.len()).unwrap_or(0));
            s.set_clip_count(i32::try_from(self.project.clips.len()).unwrap_or(0));
        }
        self.sync_inspector();
        self.sync_transport();
        self.sync_music();
        self.sync_texts();
    }

    fn strip_thumb(&mut self, id: MediaId) -> Option<slint::Image> {
        if let Some(img) = self.strip_thumbs.get(&id) {
            return Some(img.clone());
        }
        let path = self
            .library
            .request_thumb(id, ThumbLevel::Small, Priority::Soon)?;
        let img = slint::Image::load_from_path(&path).ok()?;
        self.strip_thumbs.insert(id, img.clone());
        Some(img)
    }

    fn sync_transport(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let boxes = self.display_boxes();
        s.set_playhead_x(editor_view::x_at_time(
            &self.display_clips(),
            &boxes,
            self.playhead,
        ));
        s.set_time_text(SharedString::from(format::duration(self.playhead)));
        s.set_total_text(SharedString::from(format::duration(total_duration(
            &self.project.clips,
        ))));
        s.set_playing(self.playing);
    }

    fn sync_inspector(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        // Show the first target's values (or the defaults).
        let first = self
            .targets()
            .first()
            .map(|&i| self.project.clips[i].clone());
        let (duration, fit, transition) = match first {
            Some(c) => (c.duration(), c.fit, c.transition_in),
            None => (
                self.project.settings.default_photo_duration,
                self.project.settings.default_fit,
                self.project.settings.default_transition,
            ),
        };
        #[allow(clippy::cast_possible_truncation)]
        s.set_duration_seconds(duration.as_seconds_f64() as f32);
        s.set_fit_index(if fit == Fit::Cover { 1 } else { 0 });
        s.set_transition_index(i32::try_from(transition.kind.index()).unwrap_or(0));
        if transition.kind != TransitionKind::Cut {
            #[allow(clippy::cast_possible_truncation)]
            s.set_transition_seconds(transition.duration.as_seconds_f64() as f32);
        }
        s.set_aspect_index(if self.project.settings.aspect == Aspect::Portrait9x16 {
            1
        } else {
            0
        });
        let targets = self.targets();
        let videos: Vec<&clipforge_core::Clip> = targets
            .iter()
            .map(|&i| &self.project.clips[i])
            .filter(|c| c.is_video())
            .collect();
        s.set_has_video_target(!videos.is_empty());
        s.set_only_video_target(!videos.is_empty() && videos.len() == targets.len());
        let photos = editor_view::photo_targets(&self.project, &targets);
        s.set_has_photo_target(!photos.is_empty() || self.project.clips.is_empty());
        let motion = photos
            .first()
            .map_or(self.project.settings.default_motion, |&i| {
                self.project.clips[i].motion
            });
        s.set_motion_index(i32::try_from(motion.index()).unwrap_or(0));
        s.set_transition_capped(
            transition.kind != TransitionKind::Cut
                && editor_view::transition_capped(&self.project, &targets, transition.duration),
        );
        if let Some(v) = videos.first() {
            s.set_muted(v.muted);
            s.set_volume_percent(f32::from(v.volume_percent));
        }
        self.sync_title_target();
    }

    fn sync_project(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let title = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| {
                w.global::<crate::ui::Strings>()
                    .get_untitled_project()
                    .to_string()
            });
        s.set_project_title(title.into());
        s.set_dirty(self.history.is_dirty());
        s.set_can_undo(self.history.can_undo());
        s.set_can_redo(self.history.can_redo());
        self.export_refresh();
    }

    // ----- files ------------------------------------------------------------

    fn autosave_path(&self) -> PathBuf {
        self.dirs.data.join("autosave.clipforge.json")
    }

    fn autosave(&mut self) {
        self.dirty_since = None;
        match self.project.to_json() {
            Ok(json) => {
                let path = self.autosave_path();
                let tmp = path.with_extension("tmp");
                if std::fs::write(&tmp, json)
                    .and_then(|()| std::fs::rename(&tmp, &path))
                    .is_err()
                {
                    warn!("autosave failed");
                }
            }
            Err(e) => warn!(error = %e, "autosave serialisation failed"),
        }
    }

    fn new_project(&mut self) {
        self.project = Project::new();
        self.history.clear();
        self.selection.clear();
        self.project_path = None;
        self.playhead = Ticks::ZERO;
        self.playing = false;
        self.preview_dirty = true;
        self.dirty_since = Some(Instant::now());
        self.sync_all();
    }

    fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("ClipForge project", &["clipforge.json", "json"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| Project::from_json(&t).map_err(|e| e.to_string()))
        {
            Ok(p) => {
                self.project = p;
                self.history.clear();
                self.selection.clear();
                self.project_path = Some(path);
                self.playhead = Ticks::ZERO;
                self.preview_dirty = true;
                self.dirty_since = Some(Instant::now());
                self.sync_all();
            }
            Err(e) => {
                warn!(error = %e, "open failed");
                if let Some(w) = self.state() {
                    w.global::<EditorState>().set_status_text(e.into());
                }
            }
        }
    }

    fn save_project(&mut self, ask: bool) -> bool {
        let path = match (&self.project_path, ask) {
            (Some(p), false) => p.clone(),
            _ => {
                let Some(p) = rfd::FileDialog::new()
                    .set_file_name("slideshow.clipforge.json")
                    .save_file()
                else {
                    return false;
                };
                p
            }
        };
        match self
            .project
            .to_json()
            .map_err(|e| e.to_string())
            .and_then(|j| std::fs::write(&path, j).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                self.project_path = Some(path);
                self.history.mark_saved();
                self.sync_project();
                true
            }
            Err(e) => {
                warn!(error = %e, "save failed");
                if let Some(w) = self.state() {
                    w.global::<EditorState>().set_status_text(e.into());
                }
                false
            }
        }
    }

    // ----- export -----------------------------------------------------------

    /// The dialog's options as `ExportOptions`.
    fn export_options(s: &EditorState<'_>) -> ExportOptions {
        let index = |i: i32| usize::try_from(i).unwrap_or(0);
        export_view::options(&export_view::DialogState {
            resolution: index(s.get_export_resolution_index()),
            quality: index(s.get_export_quality_index()),
            youtube: s.get_export_youtube(),
            hdr: s.get_export_hdr() && s.get_export_hdr_availability() == 0,
            codec: index(s.get_export_codec_index()),
            frame_rate: index(s.get_export_frame_rate_index()),
            video_bitrate: index(s.get_export_video_bitrate_index()),
            audio_bitrate: index(s.get_export_audio_bitrate_index()),
            container: index(s.get_export_container_index()),
        })
    }

    /// Recomputes the HDR availability and the summary line.
    fn export_refresh(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let hdr = export_view::hdr_availability(&self.project, self.worker.hdr_capable());
        s.set_export_hdr_availability(hdr.code());
        let summary = export_view::summary(&Self::export_options(&s), &self.project);
        s.set_export_size_text(format::bytes(summary.bytes).into());
        s.set_export_codec_name(summary.codec.into());
        s.set_export_video_mbps(summary.video_mbps);
        s.set_export_audio_kbps(i32::try_from(summary.audio_kbps).unwrap_or(0));
        s.set_export_auto_video_mbps(summary.auto_video_mbps);
        s.set_export_auto_audio_kbps(i32::try_from(summary.auto_audio_kbps).unwrap_or(0));
        #[allow(clippy::cast_possible_truncation)]
        s.set_export_project_fps(self.project.settings.frame_rate.as_f64() as f32);
        s.set_export_extension(summary.extension.into());
    }

    fn export_reveal(&self) {
        if let Some(path) = &self.last_export
            && let Err(e) = clipforge_platform::reveal_in_file_manager(path)
        {
            warn!(error = %e, "reveal failed");
        }
    }

    fn export_start(&mut self) {
        if self.export.is_some() || self.project.clips.is_empty() {
            return;
        }
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let options = Self::export_options(&s);
        let extension = EncodePlan::build(
            &options,
            self.project.settings.aspect,
            self.project.settings.frame_rate,
        )
        .container_extension;
        let suggested = format!(
            "{}.{extension}",
            self.project_path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map_or_else(
                    || "slideshow".to_owned(),
                    |n| n.to_string_lossy().replace(".clipforge", "")
                )
        );
        let Some(output) = rfd::FileDialog::new()
            .add_filter(extension.to_uppercase(), &[extension])
            .set_file_name(&suggested)
            .save_file()
        else {
            return;
        };
        let Some(location) = FfmpegLocation::discover() else {
            s.set_export_status(3);
            s.set_export_message(w.global::<crate::ui::Strings>().get_ffmpeg_missing());
            return;
        };
        let plan = EncodePlan::build(
            &options,
            self.project.settings.aspect,
            self.project.settings.frame_rate,
        );
        let project = self.project.clone();
        let hdr = options.hdr;
        let frame_rate = plan.frame_rate;
        let sources = FileSources::for_project(&project, self.backends.clone());
        let quality = RenderQuality::Full(options.resolution);
        let (tx, rx) = crossbeam_channel::unbounded();
        let handle = self
            .scheduler
            .submit(Priority::Interactive, "export", move |ctx| {
                let probe_cli = clipforge_media::FfmpegCli::new(location.clone());
                let exporter = match Exporter::new(location.ffmpeg) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = tx.send(ExportEvent::Finished(Err(e.to_string())));
                        return Err(JobError::Failed(e.to_string()));
                    }
                };
                // The export gets its own GPU device (or the CPU fallback).
                let compositor = clipforge_render::best_renderer();
                info!(renderer = compositor.name(), "export renderer");
                // Audio first: it is small and lets ffmpeg mux in one pass.
                let wav_path = output.with_extension("clipforge-audio.wav");
                let audio = if clipforge_export::audio::has_audio(&project) {
                    let token = ctx.token.clone();
                    let keep_going = move || !token.is_cancelled();
                    match clipforge_export::audio::write_mix(
                        &wav_path,
                        &project,
                        &sources,
                        &keep_going,
                    ) {
                        Ok(()) => Some(wav_path.clone()),
                        Err(e) => {
                            warn!(error = %e, "could not write audio mix; exporting without sound");
                            None
                        }
                    }
                } else {
                    None
                };
                let mut frames =
                    TimelineFrames::new(&project, compositor.as_ref(), &sources, quality)
                        .frame_rate(frame_rate)
                        .hdr(hdr);
                let progress_tx = tx.clone();
                let result = exporter.run(
                    &plan,
                    &mut frames,
                    audio.as_deref(),
                    &output,
                    &ctx.token,
                    |p| {
                        #[allow(clippy::cast_precision_loss)]
                        let f = if p.frames_total > 0 {
                            p.frames_sent as f32 / p.frames_total as f32
                        } else {
                            0.0
                        };
                        let _ = progress_tx.send(ExportEvent::Progress(f));
                    },
                );
                let _ = std::fs::remove_file(&wav_path);
                // Check the file against the plan before calling it done.
                let result = result.map(|report| {
                    let problems = clipforge_export::verify(
                        &probe_cli,
                        &plan,
                        report.frames,
                        audio.is_some(),
                        &report.output,
                    );
                    (report, problems.iter().map(ToString::to_string).collect())
                });
                let _ = tx.send(ExportEvent::Finished(result.map_err(|e| e.to_string())));
                Ok(())
            });
        self.export = Some(ExportRun {
            token: handle.token,
            events: rx,
            started: std::time::Instant::now(),
        });
        s.set_export_status(1);
        s.set_export_progress(0.0);
        s.set_export_minutes_left(-1);
        s.set_export_unseen(false);
        s.set_export_message(SharedString::default());
    }

    fn export_cancel(&mut self) {
        if let Some(run) = &self.export {
            run.token.cancel();
        }
    }

    fn poll_export(&mut self) {
        let Some(run) = &self.export else { return };
        let events: Vec<ExportEvent> = run.events.try_iter().collect();
        let started = run.started;
        if events.is_empty() {
            return;
        }
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        for ev in events {
            match ev {
                ExportEvent::Progress(f) => {
                    s.set_export_progress(f);
                    let left = export_view::minutes_left(started.elapsed(), f);
                    s.set_export_minutes_left(left.map_or(-1, |m| i32::try_from(m).unwrap_or(-1)));
                }
                ExportEvent::Finished(Ok((report, problems))) => {
                    info!(?report, ?problems, "export finished");
                    s.set_export_status(if problems.is_empty() { 2 } else { 4 });
                    s.set_export_progress(1.0);
                    s.set_export_output_name(
                        report
                            .output
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                            .into(),
                    );
                    s.set_export_size_text(format::bytes(report.bytes).into());
                    s.set_export_message(problems.join("; ").into());
                    s.set_export_unseen(!s.get_export_open());
                    self.last_export = Some(report.output);
                    self.export = None;
                }
                ExportEvent::Finished(Err(e)) => {
                    warn!(error = %e, "export failed");
                    s.set_export_status(3);
                    s.set_export_message(e.into());
                    s.set_export_unseen(!s.get_export_open());
                    self.export = None;
                }
            }
        }
    }
}

/// Seed for the shuffle actions: the clock, so every click gives a new mix.
fn shuffle_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| {
            u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(0)
        })
}

fn frame_to_image(frame: &Frame) -> slint::Image {
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &frame.rgba,
        frame.width,
        frame.height,
    );
    slint::Image::from_rgba8(buffer)
}

/// Replaces a model's rows in place (same length: only changed rows), so
/// Slint keeps the elements, and a gesture on one of them, alive.
pub(crate) fn update_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    if model.row_count() == rows.len() {
        for (i, row) in rows.into_iter().enumerate() {
            if model.row_data(i).as_ref() != Some(&row) {
                model.set_row_data(i, row);
            }
        }
    } else {
        model.set_vec(rows);
    }
}
