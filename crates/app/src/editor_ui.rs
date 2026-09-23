//! Glue between the Slint `EditorState` global and the core project model.
//!
//! Everything runs on the UI thread except export, which is a job. The
//! project is mutated only through `History::apply`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clipforge_core::project::{Transition, TransitionKind};
use clipforge_core::timeline::total_duration;
use clipforge_core::{
    Aspect, Command, Fit, History, MediaId, Project, ProjectSettings, Resolution, Ticks,
};
use clipforge_export::{EncodePlan, ExportOptions, Exporter, FileSources, Quality, TimelineFrames};
use clipforge_jobs::{CancellationToken, JobError, Priority, Scheduler};
use clipforge_library::{Library, ThumbLevel};
use clipforge_media::{Backends, FfmpegLocation};
use clipforge_platform::AppDirs;
use clipforge_render::{Compositor, Frame, RenderQuality, SourceImage, SourceProvider};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use tracing::{info, warn};

use crate::editor_view::{self, Selection, clips_for, layout, media_ref_for};
use crate::format;
use crate::ui::{EditorState, MainWindow, TimelineClip};

const AUTOSAVE_DELAY: Duration = Duration::from_secs(3);
const PREVIEW_MIN_INTERVAL: Duration = Duration::from_millis(33);

pub(crate) struct EditorController {
    inner: Rc<RefCell<Inner>>,
    _timer: Timer,
}

/// Preview pixels come from the library's 1280 px thumbnails.
#[derive(Default)]
struct PreviewSources {
    images: RefCell<HashMap<MediaId, SourceImage>>,
    missing: RefCell<HashSet<MediaId>>,
}

impl SourceProvider for PreviewSources {
    fn still(&self, media: MediaId, _max_edge: u32) -> Option<SourceImage> {
        let hit = self.images.borrow().get(&media).cloned();
        if hit.is_none() {
            self.missing.borrow_mut().insert(media);
        }
        hit
    }
}

enum ExportEvent {
    Progress(f32),
    Finished(Result<clipforge_export::ExportReport, String>),
}

struct ExportRun {
    token: CancellationToken,
    events: crossbeam_channel::Receiver<ExportEvent>,
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
    compositor: Compositor,
    preview: PreviewSources,
    clips_model: Rc<VecModel<TimelineClip>>,
    strip_thumbs: HashMap<MediaId, slint::Image>,
    preview_dirty: bool,
    last_preview: Instant,
    dirty_since: Option<Instant>,
    drag: Option<(usize, bool)>,
    export: Option<ExportRun>,
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

        let mut project = Project::new();
        let autosave = dirs.data.join("autosave.clipforge.json");
        if let Ok(text) = std::fs::read_to_string(&autosave)
            && let Ok(p) = Project::from_json(&text)
        {
            info!(clips = p.clips.len(), "recovered autosaved project");
            project = p;
        }

        let inner = Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            library,
            scheduler,
            backends,
            dirs,
            project,
            history: History::new(),
            selection: Selection::default(),
            project_path: None,
            playhead: Ticks::ZERO,
            playing: false,
            last_tick: Instant::now(),
            pps: editor_view::DEFAULT_PIXELS_PER_SECOND,
            compositor: Compositor::new(),
            preview: PreviewSources::default(),
            clips_model,
            strip_thumbs: HashMap::new(),
            preview_dirty: true,
            last_preview: Instant::now() - PREVIEW_MIN_INTERVAL,
            dirty_since: None,
            drag: None,
            export: None,
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
        on!(on_delete_selected, |i| i.delete_selected());
        on!(on_undo, |i| i.undo());
        on!(on_redo, |i| i.redo());
        on!(on_duration_changed, |i, secs| i.set_duration(secs));
        on!(on_fit_changed, |i, idx| i.set_fit(idx));
        on!(on_transition_changed, |i, idx, secs| i
            .set_transition(idx, secs));
        on!(on_rotate_selected, |i| i.rotate_selected());
        on!(on_aspect_changed, |i, idx| i.set_aspect(idx));
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
        let clips = clips_for(&self.project, &refs);
        let entries = clips
            .into_iter()
            .enumerate()
            .map(|(k, c)| (at + k, c))
            .collect();
        let added = refs.len();
        self.apply(Command::InsertClips {
            entries,
            media: refs,
        });
        self.set_status(
            if skipped > 0 { 2 } else { 1 },
            if skipped > 0 { skipped } else { added },
        );
        if self.playhead >= total_duration(&self.project.clips) {
            self.playhead = Ticks::ZERO;
        }
        self.preview_dirty = true;
    }

    /// Status kinds: 0 none, 1 added N, 2 skipped N (not photos / not ready).
    fn set_status(&self, kind: u8, n: usize) {
        if let Some(w) = self.state() {
            let strings = w.global::<crate::ui::Strings>();
            strings.set_count(i32::try_from(n).unwrap_or(i32::MAX));
            let text = match kind {
                1 => strings.get_added_clips(),
                2 => strings.get_skipped_items(),
                _ => SharedString::default(),
            };
            w.global::<EditorState>().set_status_text(text);
        }
    }

    fn select_all(&mut self) {
        self.selection.select_all(&self.project.clips);
        self.sync_timeline();
    }

    fn delete_selected(&mut self) {
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
            self.dirty_since.get_or_insert_with(Instant::now);
            self.preview_dirty = true;
            self.sync_all();
        }
    }

    fn redo(&mut self) {
        if self.history.redo(&mut self.project).is_some() {
            self.selection.retain_existing(&self.project.clips);
            self.dirty_since.get_or_insert_with(Instant::now);
            self.preview_dirty = true;
            self.sync_all();
        }
    }

    fn set_duration(&mut self, secs: f32) {
        let indices: Vec<usize> = self
            .targets()
            .into_iter()
            .filter(|&i| self.project.clips[i].is_photo())
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
        let kind = match idx {
            1 => TransitionKind::CrossDissolve,
            2 => TransitionKind::FadeThroughBlack,
            _ => TransitionKind::Cut,
        };
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

    // ----- selection & drag -----------------------------------------------

    fn clip_pressed(&mut self, idx: i32, shift: bool, toggle: bool) {
        let Ok(index) = usize::try_from(idx) else {
            return;
        };
        let already_selected = self
            .project
            .clips
            .get(index)
            .is_some_and(|c| self.selection.ids.contains(&c.id));
        if !(already_selected && !shift && !toggle) {
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
        let boxes = layout(&self.project.clips, self.pps);
        self.playhead = editor_view::time_at_x(&self.project.clips, &boxes, x);
        self.playing = false;
        self.preview_dirty = true;
        self.sync_transport();
    }

    fn toggle_play(&mut self) {
        if self.project.clips.is_empty() {
            return;
        }
        if !self.playing && self.playhead >= total_duration(&self.project.clips) {
            self.playhead = Ticks::ZERO;
        }
        self.playing = !self.playing;
        self.last_tick = Instant::now();
        self.sync_transport();
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
                self.playing = false;
            }
            self.preview_dirty = true;
            self.sync_transport();
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
    }

    // ----- preview ----------------------------------------------------------

    fn render_preview(&mut self) {
        self.preview_dirty = false;
        self.last_preview = Instant::now();
        self.preview.missing.borrow_mut().clear();
        let frame = self.compositor.render(
            &self.project,
            self.playhead,
            RenderQuality::Preview,
            &self.preview,
        );
        let missing: Vec<MediaId> = self.preview.missing.borrow().iter().copied().collect();
        for id in missing {
            if let Some(path) =
                self.library
                    .request_thumb(id, ThumbLevel::Preview, Priority::Interactive)
            {
                self.load_preview_image(id, &path);
            }
        }
        if let Some(w) = self.state() {
            w.global::<EditorState>()
                .set_preview(frame_to_image(&frame));
        }
    }

    fn load_preview_image(&mut self, id: MediaId, path: &std::path::Path) {
        let Ok(img) = image::open(path) else { return };
        let rgba = img.into_rgba8();
        let (w, h) = rgba.dimensions();
        self.preview.images.borrow_mut().insert(
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
        self.sync_timeline();
        self.sync_transport();
        self.sync_inspector();
        self.sync_project();
    }

    fn sync_timeline(&mut self) {
        let boxes = layout(&self.project.clips, self.pps);
        let media_ids: Vec<MediaId> = self.project.clips.iter().map(|c| c.media).collect();
        let thumbs: Vec<Option<slint::Image>> =
            media_ids.into_iter().map(|m| self.strip_thumb(m)).collect();
        let mut rows = Vec::with_capacity(self.project.clips.len());
        for (i, ((clip, b), thumb)) in self
            .project
            .clips
            .iter()
            .zip(&boxes)
            .zip(thumbs)
            .enumerate()
        {
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
                transition: match clip.transition_in.kind {
                    TransitionKind::Cut => 0,
                    TransitionKind::CrossDissolve => 1,
                    TransitionKind::FadeThroughBlack => 2,
                },
            });
        }
        // Reuse the model in place to avoid flicker.
        if self.clips_model.row_count() == rows.len() {
            for (i, row) in rows.into_iter().enumerate() {
                if self.clips_model.row_data(i).as_ref() != Some(&row) {
                    self.clips_model.set_row_data(i, row);
                }
            }
        } else {
            self.clips_model.set_vec(rows);
        }
        if let Some(w) = self.state() {
            let s = w.global::<EditorState>();
            s.set_strip_width(editor_view::strip_width(&boxes));
            s.set_selected_count(i32::try_from(self.selection.ids.len()).unwrap_or(0));
            s.set_clip_count(i32::try_from(self.project.clips.len()).unwrap_or(0));
        }
        self.sync_inspector();
        self.sync_transport();
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
        let boxes = layout(&self.project.clips, self.pps);
        s.set_playhead_x(editor_view::x_at_time(
            &self.project.clips,
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
        s.set_transition_index(match transition.kind {
            TransitionKind::Cut => 0,
            TransitionKind::CrossDissolve => 1,
            TransitionKind::FadeThroughBlack => 2,
        });
        if transition.kind != TransitionKind::Cut {
            #[allow(clippy::cast_possible_truncation)]
            s.set_transition_seconds(transition.duration.as_seconds_f64() as f32);
        }
        s.set_aspect_index(if self.project.settings.aspect == Aspect::Portrait9x16 {
            1
        } else {
            0
        });
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

    fn export_start(&mut self) {
        if self.export.is_some() || self.project.clips.is_empty() {
            return;
        }
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let options = ExportOptions {
            resolution: if s.get_export_resolution_index() == 1 {
                Resolution::Uhd4k
            } else {
                Resolution::FullHd
            },
            quality: match s.get_export_quality_index() {
                0 => Quality::Good,
                2 => Quality::Best,
                _ => Quality::Better,
            },
            hdr: false,
            optimize_for_youtube: s.get_export_youtube(),
        };
        let suggested = format!(
            "{}.mp4",
            self.project_path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map_or_else(
                    || "slideshow".to_owned(),
                    |n| n.to_string_lossy().replace(".clipforge", "")
                )
        );
        let Some(output) = rfd::FileDialog::new()
            .add_filter("MP4 video", &["mp4"])
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
        let sources = FileSources::for_project(&project, self.backends.clone());
        let quality = RenderQuality::Full(options.resolution);
        let (tx, rx) = crossbeam_channel::unbounded();
        let handle = self
            .scheduler
            .submit(Priority::Interactive, "export", move |ctx| {
                let exporter = match Exporter::new(location.ffmpeg) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = tx.send(ExportEvent::Finished(Err(e.to_string())));
                        return Err(JobError::Failed(e.to_string()));
                    }
                };
                let compositor = Compositor::new();
                let mut frames = TimelineFrames::new(&project, &compositor, &sources, quality);
                let progress_tx = tx.clone();
                let result = exporter.run(&plan, &mut frames, &output, &ctx.token, |p| {
                    #[allow(clippy::cast_precision_loss)]
                    let f = if p.frames_total > 0 {
                        p.frames_sent as f32 / p.frames_total as f32
                    } else {
                        0.0
                    };
                    let _ = progress_tx.send(ExportEvent::Progress(f));
                });
                let _ = tx.send(ExportEvent::Finished(result.map_err(|e| e.to_string())));
                Ok(())
            });
        self.export = Some(ExportRun {
            token: handle.token,
            events: rx,
        });
        s.set_export_status(1);
        s.set_export_progress(0.0);
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
        if events.is_empty() {
            return;
        }
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        for ev in events {
            match ev {
                ExportEvent::Progress(f) => s.set_export_progress(f),
                ExportEvent::Finished(Ok(report)) => {
                    info!(?report, "export finished");
                    s.set_export_status(2);
                    s.set_export_progress(1.0);
                    s.set_export_message(
                        format!(
                            "{} ({}, {})",
                            report.output.display(),
                            format::bytes(report.bytes),
                            report.encoder
                        )
                        .into(),
                    );
                    self.export = None;
                }
                ExportEvent::Finished(Err(e)) => {
                    warn!(error = %e, "export failed");
                    s.set_export_status(3);
                    s.set_export_message(e.into());
                    self.export = None;
                }
            }
        }
    }
}

fn frame_to_image(frame: &Frame) -> slint::Image {
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &frame.rgba,
        frame.width,
        frame.height,
    );
    slint::Image::from_rgba8(buffer)
}
