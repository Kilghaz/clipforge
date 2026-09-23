//! Glue between the Slint `LibraryState` global and `clipforge_library`.
//!
//! Everything here runs on the UI thread. Background work happens inside
//! the library; results arrive as events that a timer drains.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clipforge_core::MediaId;
use clipforge_jobs::Priority;
use clipforge_library::{Library, LibraryEvent, MediaRecord, ProbeState, ThumbLevel};
use clipforge_media::MediaKind;
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use tracing::{debug, warn};

use crate::format;
use crate::library_view::{self, ThumbWindow, ViewState, columns_for_width, row_count, row_of};
use crate::ui::{GridRow, InspectorInfo, LibraryState, MainWindow, MediaCell};

/// How many cells keep a decoded thumbnail in memory.
const THUMB_WINDOW: usize = 800;
/// Minimum interval between grid rebuilds while events stream in.
const REFRESH_INTERVAL: Duration = Duration::from_millis(150);

pub(crate) struct LibraryController {
    inner: Rc<RefCell<Inner>>,
    _timer: Timer,
}

struct Inner {
    library: Arc<Library>,
    window: slint::Weak<MainWindow>,
    view: ViewState,
    ids: Vec<MediaId>,
    index_of: HashMap<MediaId, usize>,
    columns: usize,
    rows: Rc<VecModel<GridRow>>,
    row_models: Vec<Rc<VecModel<MediaCell>>>,
    thumbs: ThumbWindow,
    selected: Option<MediaId>,
    dirty: bool,
    last_refresh: Instant,
}

impl LibraryController {
    pub(crate) fn new(window: &MainWindow, library: Arc<Library>) -> Self {
        let rows = Rc::new(VecModel::default());
        let inner = Rc::new(RefCell::new(Inner {
            library,
            window: window.as_weak(),
            view: ViewState::default(),
            ids: Vec::new(),
            index_of: HashMap::new(),
            columns: 1,
            rows: Rc::clone(&rows),
            row_models: Vec::new(),
            thumbs: ThumbWindow::new(THUMB_WINDOW),
            selected: None,
            dirty: true,
            last_refresh: Instant::now() - REFRESH_INTERVAL,
        }));
        let state = window.global::<LibraryState>();
        state.set_rows(ModelRc::from(rows));

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
        on!(on_add_files, |i| i.add_files());
        on!(on_add_folder, |i| i.add_folder());
        on!(on_search_changed, |i, text| {
            i.view.search = text.to_string();
            i.mark_dirty();
        });
        on!(on_filter_changed, |i, idx| {
            i.view.filter = library_view::KindFilter::from_index(idx);
            i.mark_dirty();
        });
        on!(on_sort_changed, |i, idx| {
            let sort = library_view::SortChoice::from_index(idx);
            i.view.sort = sort;
            i.view.descending = sort.default_descending();
            i.sync_toolbar();
            i.mark_dirty();
        });
        on!(on_direction_toggled, |i| {
            i.view.descending = !i.view.descending;
            i.sync_toolbar();
            i.mark_dirty();
        });
        on!(on_cell_clicked, |i, idx| i
            .select(usize::try_from(idx).ok()));
        on!(on_cell_shown, |i, idx| {
            if let Ok(idx) = usize::try_from(idx) {
                i.cell_shown(idx);
            }
        });
        on!(on_grid_width_changed, |i, width| i.set_width(width));
        on!(on_remove_selected, |i| i.remove_selected());
        on!(on_reveal_selected, |i| i.reveal_selected());

        let timer = Timer::default();
        {
            let inner = Rc::clone(&inner);
            timer.start(TimerMode::Repeated, Duration::from_millis(40), move || {
                let mut i = inner.borrow_mut();
                i.pump_events();
                if i.dirty && i.last_refresh.elapsed() >= REFRESH_INTERVAL {
                    i.refresh();
                }
            });
        }
        inner.borrow_mut().refresh();
        LibraryController {
            inner,
            _timer: timer,
        }
    }

    /// Starts an import of the given paths (used by menu actions and tests).
    pub(crate) fn import(&self, paths: Vec<PathBuf>) {
        self.inner.borrow().start_import(paths);
    }
}

impl Inner {
    fn state(&self) -> Option<MainWindow> {
        self.window.upgrade()
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn sync_toolbar(&self) {
        if let Some(w) = self.state() {
            let s = w.global::<LibraryState>();
            s.set_descending(self.view.descending);
        }
    }

    fn set_width(&mut self, width: f32) {
        let columns = columns_for_width(width);
        if columns != self.columns {
            self.columns = columns;
            self.rebuild_rows();
        }
    }

    /// Re-queries the catalogue and rebuilds the grid.
    fn refresh(&mut self) {
        self.dirty = false;
        self.last_refresh = Instant::now();
        let query = self.view.query();
        let ids = match self.library.catalogue().query_ids(&query) {
            Ok(ids) => ids,
            Err(e) => {
                warn!(error = %e, "library query failed");
                return;
            }
        };
        self.index_of = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        self.ids = ids;
        let total = self.library.catalogue().len().unwrap_or(0);
        if let Some(w) = self.state() {
            w.global::<LibraryState>()
                .set_total_count(i32::try_from(total).unwrap_or(i32::MAX));
        }
        self.rebuild_rows();
        if let Some(sel) = self.selected
            && !self.index_of.contains_key(&sel)
        {
            self.select(None);
        }
    }

    /// Creates empty cells; content is filled lazily when a cell is shown.
    fn rebuild_rows(&mut self) {
        self.thumbs.clear();
        let n_rows = row_count(self.ids.len(), self.columns);
        let mut row_models = Vec::with_capacity(n_rows);
        let mut grid_rows = Vec::with_capacity(n_rows);
        for r in 0..n_rows {
            let start = r * self.columns;
            let end = (start + self.columns).min(self.ids.len());
            let cells: Vec<MediaCell> = (start..end)
                .map(|i| MediaCell {
                    index: i32::try_from(i).unwrap_or(0),
                    id: SharedString::from(self.ids[i].to_string()),
                    pending: true,
                    selected: self.selected == Some(self.ids[i]),
                    ..Default::default()
                })
                .collect();
            let model = Rc::new(VecModel::from(cells));
            grid_rows.push(GridRow {
                cells: ModelRc::from(Rc::clone(&model)),
            });
            row_models.push(model);
        }
        self.row_models = row_models;
        self.rows.set_vec(grid_rows);
    }

    fn cell(&self, index: usize) -> Option<(Rc<VecModel<MediaCell>>, usize, MediaCell)> {
        let row = self.row_models.get(row_of(index, self.columns))?;
        let col = index % self.columns.max(1);
        let cell = row.row_data(col)?;
        Some((Rc::clone(row), col, cell))
    }

    fn update_cell(&self, index: usize, f: impl FnOnce(&mut MediaCell)) {
        if let Some((row, col, mut cell)) = self.cell(index) {
            f(&mut cell);
            row.set_row_data(col, cell);
        }
    }

    fn cell_shown(&mut self, index: usize) {
        let Some(id) = self.ids.get(index).copied() else {
            return;
        };
        for old in self.thumbs.shown(index) {
            self.update_cell(old, |c| {
                c.image = slint::Image::default();
                c.has_image = false;
            });
            if let Some(old_id) = self.ids.get(old) {
                self.library.demote_thumb(*old_id, ThumbLevel::Small);
            }
        }
        self.populate(index, id);
    }

    /// Fills a cell from its catalogue record and requests its thumbnail.
    fn populate(&mut self, index: usize, id: MediaId) {
        let Ok(record) = self.library.catalogue().get(id) else {
            return;
        };
        // The library itself skips audio, placeholders and failed items; we
        // only avoid asking while the probe is still pending.
        let thumb = if record.probe == ProbeState::Pending {
            None
        } else {
            self.library
                .request_thumb(id, ThumbLevel::Small, Priority::Interactive)
        };
        let image = thumb.and_then(|p| slint::Image::load_from_path(&p).ok());
        self.update_cell(index, |c| {
            fill_cell(c, &record);
            if let Some(img) = image {
                c.image = img;
                c.has_image = true;
            }
        });
    }

    fn select(&mut self, index: Option<usize>) {
        let new_id = index.and_then(|i| self.ids.get(i).copied());
        if let Some(old) = self.selected.and_then(|id| self.index_of.get(&id).copied()) {
            self.update_cell(old, |c| c.selected = false);
        }
        if let Some(i) = index {
            self.update_cell(i, |c| c.selected = true);
        }
        self.selected = new_id;
        self.refresh_inspector();
    }

    fn refresh_inspector(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<LibraryState>();
        let record = self
            .selected
            .and_then(|id| self.library.catalogue().get(id).ok());
        match record {
            None => {
                s.set_inspector(InspectorInfo::default());
                s.set_inspector_has_image(false);
            }
            Some(record) => {
                s.set_inspector(inspector_info(&record));
                let path = self.library.request_thumb(
                    record.id,
                    ThumbLevel::Medium,
                    Priority::Interactive,
                );
                match path.and_then(|p| slint::Image::load_from_path(&p).ok()) {
                    Some(img) => {
                        s.set_inspector_image(img);
                        s.set_inspector_has_image(true);
                    }
                    None => s.set_inspector_has_image(false),
                }
            }
        }
    }

    fn pump_events(&mut self) {
        let events: Vec<LibraryEvent> = self.library.events().try_iter().collect();
        if events.is_empty() {
            return;
        }
        let Some(w) = self.state() else { return };
        let s = w.global::<LibraryState>();
        for ev in events {
            match ev {
                LibraryEvent::ImportStarted { total } => {
                    s.set_status_kind(1);
                    s.set_import_total(to_i32(total));
                    s.set_import_done(0);
                }
                LibraryEvent::ImportProgress { done, total } => {
                    s.set_status_kind(2);
                    s.set_import_done(to_i32(done));
                    s.set_import_total(to_i32(total));
                }
                LibraryEvent::ItemsAdded(_) => self.dirty = true,
                LibraryEvent::ImportFinished {
                    added,
                    existing,
                    failed,
                } => {
                    debug!(added, existing, failed, "import finished");
                    s.set_status_kind(3);
                    s.set_import_added(to_i32(added));
                    s.set_import_existing(to_i32(existing));
                    self.dirty = true;
                }
                LibraryEvent::ItemUpdated(id) => {
                    if let Some(&index) = self.index_of.get(&id)
                        && self.thumbs.contains(index)
                    {
                        self.populate(index, id);
                    }
                    if self.selected == Some(id) {
                        self.refresh_inspector();
                    }
                    // Sorting by date may change once capture time is known.
                    self.dirty = true;
                }
                LibraryEvent::ThumbReady { id, level, path } => match level {
                    ThumbLevel::Small => {
                        if let Some(&index) = self.index_of.get(&id)
                            && self.thumbs.contains(index)
                            && let Ok(img) = slint::Image::load_from_path(&path)
                        {
                            self.update_cell(index, |c| {
                                c.image = img;
                                c.has_image = true;
                                c.pending = false;
                            });
                        }
                    }
                    ThumbLevel::Medium | ThumbLevel::Preview => {
                        if self.selected == Some(id)
                            && let Ok(img) = slint::Image::load_from_path(&path)
                        {
                            s.set_inspector_image(img);
                            s.set_inspector_has_image(true);
                        }
                    }
                },
                LibraryEvent::ThumbFailed { id, level, message } => {
                    debug!(%id, ?level, message, "thumbnail failed");
                    if let Some(&index) = self.index_of.get(&id) {
                        self.update_cell(index, |c| c.pending = false);
                    }
                }
            }
        }
    }

    fn start_import(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        debug!(count = paths.len(), "import requested");
        self.library.import(paths);
    }

    fn add_files(&self) {
        let exts: Vec<&str> = MediaKind::supported_extensions().collect();
        let picked = rfd::FileDialog::new()
            .add_filter("Media", &exts)
            .pick_files()
            .unwrap_or_default();
        self.start_import(picked);
    }

    fn add_folder(&self) {
        let picked = rfd::FileDialog::new().pick_folders().unwrap_or_default();
        self.start_import(picked);
    }

    fn remove_selected(&mut self) {
        if let Some(id) = self.selected {
            if let Err(e) = self.library.remove(&[id]) {
                warn!(error = %e, "remove failed");
            }
            self.selected = None;
            self.refresh();
            self.refresh_inspector();
        }
    }

    fn reveal_selected(&self) {
        if let Some(record) = self
            .selected
            .and_then(|id| self.library.catalogue().get(id).ok())
            && let Err(e) = clipforge_platform::reveal_in_file_manager(&record.path)
        {
            warn!(error = %e, "reveal failed");
        }
    }
}

fn to_i32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn kind_code(kind: MediaKind) -> i32 {
    match kind {
        MediaKind::Photo => 0,
        MediaKind::Video => 1,
        MediaKind::Audio => 2,
        MediaKind::Unknown => 3,
    }
}

fn fill_cell(c: &mut MediaCell, r: &MediaRecord) {
    c.title = SharedString::from(r.file_name());
    c.kind = kind_code(r.kind);
    c.cloud = r.cloud_state == clipforge_library::CloudState::Placeholder;
    c.failed = matches!(r.probe, ProbeState::Failed(_));
    c.pending = r.probe == ProbeState::Pending && !c.cloud;
    c.subtitle = SharedString::from(match &r.info {
        Some(info) => match (info.duration, info.display_size()) {
            (Some(d), _) => format::duration(d),
            (None, Some((w, h))) => format::dimensions(w, h),
            _ => String::new(),
        },
        None => String::new(),
    });
}

fn inspector_info(r: &MediaRecord) -> InspectorInfo {
    let info = r.info.as_ref();
    let status_kind = if r.cloud_state == clipforge_library::CloudState::Placeholder {
        3
    } else {
        match r.probe {
            ProbeState::Pending => 0,
            ProbeState::Done => 1,
            ProbeState::Failed(_) => 2,
        }
    };
    InspectorInfo {
        visible: true,
        title: SharedString::from(r.file_name()),
        kind: kind_code(r.kind),
        dimensions: SharedString::from(
            info.and_then(|i| i.display_size())
                .map(|(w, h)| format::dimensions(w, h))
                .unwrap_or_default(),
        ),
        duration: SharedString::from(
            info.and_then(|i| i.duration)
                .map(format::duration)
                .unwrap_or_default(),
        ),
        captured: SharedString::from(format::date_time_utc(r.sort_time_ms())),
        codec: SharedString::from(info.map(|i| i.codec.clone()).unwrap_or_default()),
        size: SharedString::from(format::bytes(r.fingerprint.size)),
        path: SharedString::from(r.path.display().to_string()),
        status_kind,
        error: SharedString::from(match &r.probe {
            ProbeState::Failed(m) => m.clone(),
            _ => String::new(),
        }),
        hdr: info.is_some_and(|i| i.is_hdr()),
    }
}
