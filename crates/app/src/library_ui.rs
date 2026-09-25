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
use crate::library_view::{
    self, GridNav, GridSelection, KeyMode, ThumbWindow, ViewState, columns_for_width, grid_nav,
    row_count, row_of,
};
use crate::ui::{GridRow, InspectorInfo, LibraryState, MainWindow, MediaCell, Shell};

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
    /// Decoded small thumbnails for cells inside the thumb window.
    images: HashMap<MediaId, slint::Image>,
    selection: GridSelection,
    /// Cell that currently carries `focused` in the model.
    shown_focus: Option<usize>,
    dirty: bool,
    last_refresh: Instant,
    /// Editor hook: add these library items to the timeline.
    add_to_timeline: Option<Rc<dyn Fn(Vec<MediaId>)>>,
    /// Editor hook: a preview-size thumbnail is ready.
    preview_ready: Option<Rc<dyn Fn(MediaId, PathBuf)>>,
    /// Editor hook: drag hover over the strip (`Some(x)` in strip content px) or leave.
    drop_hover: Option<Rc<dyn Fn(Option<f32>)>>,
    /// Editor hook: drop these items at strip content x.
    drop_insert: Option<DropInsertHook>,
    /// Press-and-drag state.
    press: Option<Press>,
    marquee: Option<Marquee>,
}

/// Rubber-band selection in progress.
#[derive(Clone)]
struct Marquee {
    /// Start point in grid coordinates.
    start: (f32, f32),
    /// Selection when the marquee began (kept when additive).
    base: std::collections::HashSet<MediaId>,
    additive: bool,
}

/// Callback that inserts library items into the timeline at a strip x.
type DropInsertHook = Rc<dyn Fn(Vec<MediaId>, f32)>;

struct Press {
    index: usize,
    start: Option<(f32, f32)>,
    dragging: bool,
    /// Whether the press should turn into a plain select on release.
    select_on_release: bool,
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
            images: HashMap::new(),
            selection: GridSelection::default(),
            shown_focus: None,
            dirty: true,
            last_refresh: Instant::now() - REFRESH_INTERVAL,
            add_to_timeline: None,
            preview_ready: None,
            drop_hover: None,
            drop_insert: None,
            press: None,
            marquee: None,
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
        on!(on_cell_pressed, |i, idx, shift, toggle| {
            if let Ok(idx) = usize::try_from(idx) {
                i.cell_pressed(idx, shift, toggle);
            }
        });
        on!(on_cell_drag_moved, |i, idx, x, y| {
            if let Ok(idx) = usize::try_from(idx) {
                i.cell_drag_moved(idx, x, y);
            }
        });
        on!(on_cell_released, |i, idx, x, y| {
            if let Ok(idx) = usize::try_from(idx) {
                i.cell_released(idx, x, y);
            }
        });
        on!(on_marquee_start, |i, x, y, additive| i
            .marquee_start(x, y, additive));
        on!(on_marquee_move, |i, x, y| i.marquee_move(x, y));
        on!(on_marquee_end, |i| i.marquee_end());
        on!(on_select_all, |i| i.select_all());
        on!(on_clear_selection, |i| i.clear_selection());
        on!(on_cancel_drag, |i| i.cancel_drag());
        on!(on_grid_navigate, |i, nav, mode, page_rows| i
            .grid_navigate(nav, mode, page_rows));
        on!(on_grid_toggle_focused, |i| i.grid_toggle_focused());
        on!(on_grid_activate, |i| i.grid_activate());
        on!(on_grid_focus_entered, |i| i.grid_focus_entered());
        on!(on_cell_shown, |i, idx| {
            if let Ok(idx) = usize::try_from(idx) {
                i.cell_shown(idx);
            }
        });
        on!(on_grid_width_changed, |i, width| i.set_width(width));
        on!(on_remove_selected, |i| i.remove_selected());
        on!(on_reveal_selected, |i| i.reveal_selected());
        on!(on_add_selected_to_timeline, |i| {
            let ids = i.selection.ordered(&i.ids);
            if let (false, Some(hook)) = (ids.is_empty(), i.add_to_timeline.clone()) {
                hook(ids);
            }
        });
        on!(on_add_all_to_timeline, |i| {
            if let Some(hook) = i.add_to_timeline.clone() {
                hook(i.ids.clone());
            }
        });
        on!(on_cell_double_clicked, |i, idx| {
            if let (Ok(idx), Some(hook)) = (usize::try_from(idx), i.add_to_timeline.clone())
                && let Some(id) = i.ids.get(idx)
            {
                hook(vec![*id]);
            }
        });

        let timer = Timer::default();
        {
            let inner = Rc::clone(&inner);
            timer.start(TimerMode::Repeated, Duration::from_millis(40), move || {
                let mut i = inner.borrow_mut();
                i.poll_width();
                i.pump_events();
                if i.dirty && i.last_refresh.elapsed() >= REFRESH_INTERVAL {
                    i.refresh();
                }
            });
        }
        {
            let mut i = inner.borrow_mut();
            i.set_width(state.get_grid_width());
            i.refresh();
        }
        LibraryController {
            inner,
            _timer: timer,
        }
    }

    /// Starts an import of the given paths (used by menu actions and tests).
    pub(crate) fn import(&self, paths: Vec<PathBuf>) {
        self.inner.borrow().start_import(paths);
    }

    /// Connects the editor: adding items to the timeline, preview thumbs and
    /// drag-and-drop into the strip.
    pub(crate) fn connect_editor(
        &self,
        add: Rc<dyn Fn(Vec<MediaId>)>,
        preview_ready: Rc<dyn Fn(MediaId, PathBuf)>,
        drop_hover: Rc<dyn Fn(Option<f32>)>,
        drop_insert: DropInsertHook,
    ) {
        let mut i = self.inner.borrow_mut();
        i.add_to_timeline = Some(add);
        i.preview_ready = Some(preview_ready);
        i.drop_hover = Some(drop_hover);
        i.drop_insert = Some(drop_insert);
    }

    /// A cloneable closure that starts an import; for event hooks that
    /// outlive borrows of the controller.
    pub(crate) fn import_handle(&self) -> Rc<dyn Fn(Vec<PathBuf>)> {
        let inner = Rc::clone(&self.inner);
        Rc::new(move |paths| inner.borrow().start_import(paths))
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

    /// Picks up layout changes that arrive without a `changed` callback
    /// (initial layout, panel shown again).
    fn poll_width(&mut self) {
        if let Some(w) = self.state() {
            let width = w.global::<LibraryState>().get_grid_width();
            if width > 0.0 {
                self.set_width(width);
            }
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
        self.selection.retain_existing(&self.ids);
        self.rebuild_rows();
        self.sync_selection_count();
        self.refresh_inspector();
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
                .map(|i| {
                    let id = self.ids[i];
                    let image = self.images.get(&id).cloned();
                    MediaCell {
                        index: i32::try_from(i).unwrap_or(0),
                        id: SharedString::from(id.to_string()),
                        pending: image.is_none(),
                        has_image: image.is_some(),
                        image: image.unwrap_or_default(),
                        selected: self.selection.ids.contains(&id),
                        focused: self.selection.focus == Some(i),
                        ..Default::default()
                    }
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
        self.shown_focus = self.selection.focus;
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
                self.images.remove(old_id);
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
        let image = match self.images.get(&id) {
            Some(img) => Some(img.clone()),
            None => {
                let loaded = thumb.and_then(|p| slint::Image::load_from_path(&p).ok());
                if let Some(img) = &loaded {
                    self.images.insert(id, img.clone());
                }
                loaded
            }
        };
        let selected = self.selection.ids.contains(&id);
        let focused = self.selection.focus == Some(index);
        self.update_cell(index, |c| {
            fill_cell(c, &record);
            c.selected = selected;
            c.focused = focused;
            if let Some(img) = image {
                c.image = img;
                c.has_image = true;
            }
        });
    }

    // ----- selection, marquee, drag ------------------------------------------

    fn set_selection_from(&mut self, before: &std::collections::HashSet<MediaId>) {
        let changed: Vec<MediaId> = before
            .symmetric_difference(&self.selection.ids)
            .copied()
            .collect();
        for id in changed {
            if let Some(&index) = self.index_of.get(&id) {
                let on = self.selection.ids.contains(&id);
                self.update_cell(index, |c| c.selected = on);
            }
        }
        self.sync_focus();
        self.sync_selection_count();
        self.refresh_inspector();
    }

    /// Moves the `focused` flag in the model to the selection's focus.
    fn sync_focus(&mut self) {
        let focus = self.selection.focus;
        if focus == self.shown_focus {
            return;
        }
        if let Some(old) = self.shown_focus {
            self.update_cell(old, |c| c.focused = false);
        }
        if let Some(new) = focus {
            self.update_cell(new, |c| c.focused = true);
        }
        self.shown_focus = focus;
    }

    /// Tells the grid which row to scroll into view.
    fn reveal_focus(&self) {
        let (Some(w), Some(f)) = (self.state(), self.selection.focus) else {
            return;
        };
        let s = w.global::<LibraryState>();
        s.set_focused_row(to_i32(row_of(f, self.columns)));
        s.set_focus_token(s.get_focus_token().wrapping_add(1));
    }

    fn grid_navigate(&mut self, nav: i32, mode: i32, page_rows: i32) {
        let nav = match nav {
            0 => GridNav::Left,
            1 => GridNav::Right,
            2 => GridNav::Up,
            3 => GridNav::Down,
            4 => GridNav::Home,
            5 => GridNav::End,
            6 => GridNav::PageUp,
            _ => GridNav::PageDown,
        };
        let mode = match mode {
            1 => KeyMode::Extend,
            2 => KeyMode::FocusOnly,
            _ => KeyMode::Replace,
        };
        // The first key press after entering only lands on the entry item.
        let target = match self.selection.focus {
            Some(from) => grid_nav(
                from,
                self.ids.len(),
                self.columns,
                usize::try_from(page_rows).unwrap_or(1),
                nav,
            ),
            None => self.selection.entry_focus(&self.ids),
        };
        let Some(target) = target else { return };
        let before = self.selection.ids.clone();
        self.selection.key_select(&self.ids, target, mode);
        self.set_selection_from(&before);
        self.reveal_focus();
    }

    fn grid_toggle_focused(&mut self) {
        if self.selection.focus.is_none() {
            self.selection.focus = self.selection.entry_focus(&self.ids);
        }
        let before = self.selection.ids.clone();
        self.selection.toggle_focused(&self.ids);
        self.set_selection_from(&before);
        self.reveal_focus();
    }

    /// Enter: the selection goes to the timeline; with nothing selected,
    /// the focused item does.
    fn grid_activate(&mut self) {
        let mut ids = self.selection.ordered(&self.ids);
        if ids.is_empty()
            && let Some(id) = self.selection.focus.and_then(|f| self.ids.get(f))
        {
            ids.push(*id);
        }
        if let (false, Some(hook)) = (ids.is_empty(), self.add_to_timeline.clone()) {
            hook(ids);
        }
    }

    fn grid_focus_entered(&mut self) {
        if self.selection.focus.is_none() {
            self.selection.focus = self.selection.entry_focus(&self.ids);
        }
        self.sync_focus();
        self.reveal_focus();
    }

    fn sync_selection_count(&self) {
        if let Some(w) = self.state() {
            w.global::<LibraryState>()
                .set_selected_count(to_i32(self.selection.ids.len()));
        }
    }

    fn cell_pressed(&mut self, index: usize, shift: bool, toggle: bool) {
        let before = self.selection.ids.clone();
        let already = self
            .ids
            .get(index)
            .is_some_and(|id| self.selection.ids.contains(id));
        let mut select_on_release = false;
        if shift || toggle {
            self.selection.click(&self.ids, index, shift, toggle);
        } else if already {
            // Keep the group so it can be dragged; plain-select on release if no drag happens.
            select_on_release = true;
        } else {
            self.selection.click(&self.ids, index, false, false);
        }
        self.selection.focus = Some(index);
        self.set_selection_from(&before);
        self.press = Some(Press {
            index,
            start: None,
            dragging: false,
            select_on_release,
        });
    }

    fn cell_drag_moved(&mut self, index: usize, x: f32, y: f32) {
        let Some(press) = self.press.as_mut() else {
            return;
        };
        if press.index != index {
            return;
        }
        let start = *press.start.get_or_insert((x, y));
        if !press.dragging {
            if (x - start.0).abs().max((y - start.1).abs()) < library_view::DRAG_THRESHOLD {
                return;
            }
            press.dragging = true;
            press.select_on_release = false;
        }
        let count = self.selection.ordered(&self.ids).len();
        let over = self.strip_x_for(x, y);
        if let Some(w) = self.state() {
            let sh = w.global::<Shell>();
            sh.set_drag_active(true);
            sh.set_drag_x(x);
            sh.set_drag_y(y);
            sh.set_drag_count(to_i32(count));
            sh.set_drag_over_timeline(over.is_some());
        }
        if let Some(hook) = self.drop_hover.clone() {
            hook(over);
        }
    }

    fn cell_released(&mut self, index: usize, x: f32, y: f32) {
        let Some(press) = self.press.take() else {
            return;
        };
        if press.index != index {
            return;
        }
        if press.dragging {
            if let Some(w) = self.state() {
                w.global::<Shell>().set_drag_active(false);
            }
            if let Some(hook) = self.drop_hover.clone() {
                hook(None);
            }
            if let (Some(strip_x), Some(insert)) =
                (self.strip_x_for(x, y), self.drop_insert.clone())
            {
                let ids = self.selection.ordered(&self.ids);
                if !ids.is_empty() {
                    insert(ids, strip_x);
                }
            }
        } else if press.select_on_release {
            let before = self.selection.ids.clone();
            self.selection.click(&self.ids, index, false, false);
            self.set_selection_from(&before);
        }
    }

    /// If the window point is over the timeline strip, its x in strip content pixels.
    fn strip_x_for(&self, x: f32, y: f32) -> Option<f32> {
        let w = self.state()?;
        let r = w.get_editor_strip_rect();
        (x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height)
            .then_some(x - r.x + r.scroll)
    }

    fn marquee_start(&mut self, x: f32, y: f32, additive: bool) {
        let before = self.selection.ids.clone();
        if !additive {
            self.selection.clear();
            self.set_selection_from(&before);
        }
        self.marquee = Some(Marquee {
            start: (x, y),
            base: self.selection.ids.clone(),
            additive,
        });
        if let Some(w) = self.state() {
            w.global::<LibraryState>().set_marquee_visible(false);
        }
    }

    fn marquee_move(&mut self, x: f32, y: f32) {
        let Some(m) = self.marquee.clone() else {
            return;
        };
        let (sx, sy) = m.start;
        let hits = library_view::cells_in_rect(self.ids.len(), self.columns, m.start, (x, y));
        let before = self.selection.ids.clone();
        self.selection
            .marquee(&self.ids, &hits, &m.base, m.additive);
        self.set_selection_from(&before);
        if let Some(w) = self.state() {
            let s = w.global::<LibraryState>();
            s.set_marquee_visible(true);
            s.set_marquee_x(sx.min(x));
            s.set_marquee_y(sy.min(y));
            s.set_marquee_w((x - sx).abs());
            s.set_marquee_h((y - sy).abs());
        }
    }

    fn marquee_end(&mut self) {
        self.marquee = None;
        if let Some(w) = self.state() {
            w.global::<LibraryState>().set_marquee_visible(false);
        }
    }

    /// Ctrl/Cmd+A in the library: select every visible item.
    fn select_all(&mut self) {
        let before = self.selection.ids.clone();
        self.selection.ids.extend(self.ids.iter().copied());
        self.set_selection_from(&before);
    }

    /// Escape in the library: drop the selection.
    fn clear_selection(&mut self) {
        let before = self.selection.ids.clone();
        self.selection.clear();
        self.set_selection_from(&before);
    }

    /// Escape during a drag: abandon it without dropping anything.
    fn cancel_drag(&mut self) {
        let Some(press) = self.press.take() else {
            return;
        };
        if press.dragging {
            if let Some(w) = self.state() {
                w.global::<Shell>().set_drag_active(false);
            }
            if let Some(hook) = self.drop_hover.clone() {
                hook(None);
            }
        }
        self.marquee_end();
    }

    fn refresh_inspector(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<LibraryState>();
        let single = if self.selection.ids.len() == 1 {
            self.selection.ids.iter().next().copied()
        } else {
            None
        };
        let record = single.and_then(|id| self.library.catalogue().get(id).ok());
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
                    if self.selection.ids.len() == 1 && self.selection.ids.contains(&id) {
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
                            self.images.insert(id, img.clone());
                            self.update_cell(index, |c| {
                                c.image = img;
                                c.has_image = true;
                                c.pending = false;
                            });
                        }
                    }
                    ThumbLevel::Medium => {
                        if self.selection.ids.len() == 1
                            && self.selection.ids.contains(&id)
                            && let Ok(img) = slint::Image::load_from_path(&path)
                        {
                            s.set_inspector_image(img);
                            s.set_inspector_has_image(true);
                        }
                    }
                    ThumbLevel::Preview => {
                        if let Some(hook) = self.preview_ready.clone() {
                            hook(id, path);
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
        let ids = self.selection.ordered(&self.ids);
        if ids.is_empty() {
            return;
        }
        if let Err(e) = self.library.remove(&ids) {
            warn!(error = %e, "remove failed");
        }
        self.selection.clear();
        self.refresh();
    }

    fn reveal_selected(&self) {
        if let Some(record) = self
            .selection
            .ids
            .iter()
            .next()
            .and_then(|id| self.library.catalogue().get(*id).ok())
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
