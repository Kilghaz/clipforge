//! Pure view logic for the library grid: layout maths, query state and the
//! window of rows whose thumbnails we keep in memory. No Slint here.

use std::collections::VecDeque;

use clipforge_library::{Query, Sort};
use clipforge_media::MediaKind;

/// Width of one grid cell including its gap, in logical pixels. Must match
/// `CellView` (104 px) plus the row spacing (8 px) in `library.slint`.
pub(crate) const CELL_WIDTH: f32 = 112.0;
/// Visible width of a cell (without the gap).
pub(crate) const CELL_VISIBLE_WIDTH: f32 = 104.0;
/// Height of a grid row; must match the row height in `library.slint`.
pub(crate) const ROW_HEIGHT: f32 = 120.0;
/// Visible height of a cell.
pub(crate) const CELL_VISIBLE_HEIGHT: f32 = 112.0;
/// Left padding of the grid, in logical pixels.
pub(crate) const GRID_PADDING: f32 = 12.0;
/// Pointer travel before a press turns into a drag.
pub(crate) const DRAG_THRESHOLD: f32 = 6.0;

/// Number of columns that fit into `width`. Never below one.
#[must_use]
pub(crate) fn columns_for_width(width: f32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((width - 2.0 * GRID_PADDING) / CELL_WIDTH).floor().max(1.0) as usize;
    n
}

#[must_use]
pub(crate) fn row_count(items: usize, columns: usize) -> usize {
    items.div_ceil(columns.max(1))
}

#[must_use]
pub(crate) fn row_of(index: usize, columns: usize) -> usize {
    index / columns.max(1)
}

/// Filter entries as shown in the kind picker, in order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum KindFilter {
    All,
    Photos,
    Videos,
    Audio,
}

impl KindFilter {
    pub(crate) const ALL: [KindFilter; 4] = [
        KindFilter::All,
        KindFilter::Photos,
        KindFilter::Videos,
        KindFilter::Audio,
    ];

    #[must_use]
    pub(crate) fn from_index(i: i32) -> KindFilter {
        usize::try_from(i)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(KindFilter::All)
    }

    fn kinds(self) -> Option<Vec<MediaKind>> {
        match self {
            KindFilter::All => None,
            KindFilter::Photos => Some(vec![MediaKind::Photo]),
            KindFilter::Videos => Some(vec![MediaKind::Video]),
            KindFilter::Audio => Some(vec![MediaKind::Audio]),
        }
    }
}

/// Sort entries as shown in the sort picker, in order.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum SortChoice {
    Date,
    Name,
    Kind,
    Added,
}

impl SortChoice {
    pub(crate) const ALL: [SortChoice; 4] = [
        SortChoice::Date,
        SortChoice::Name,
        SortChoice::Kind,
        SortChoice::Added,
    ];

    #[must_use]
    pub(crate) fn from_index(i: i32) -> SortChoice {
        usize::try_from(i)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(SortChoice::Date)
    }

    fn sort(self) -> Sort {
        match self {
            SortChoice::Date => Sort::CapturedAt,
            SortChoice::Name => Sort::Name,
            SortChoice::Kind => Sort::Kind,
            SortChoice::Added => Sort::AddedAt,
        }
    }

    /// Dates and additions read best newest-first; names A to Z.
    #[must_use]
    pub(crate) fn default_descending(self) -> bool {
        matches!(self, SortChoice::Date | SortChoice::Added)
    }
}

/// What the toolbar currently says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ViewState {
    pub search: String,
    pub filter: KindFilter,
    pub sort: SortChoice,
    pub descending: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            search: String::new(),
            filter: KindFilter::All,
            sort: SortChoice::Date,
            descending: true,
        }
    }
}

impl ViewState {
    #[must_use]
    pub(crate) fn query(&self) -> Query {
        let mut q = Query::all()
            .text(self.search.clone())
            .sort(self.sort.sort(), self.descending);
        q.kinds = self.filter.kinds();
        q
    }
}

/// Cell rectangle in grid coordinates (origin at the top-left of row 0).
#[must_use]
pub(crate) fn cell_rect(index: usize, columns: usize) -> (f32, f32, f32, f32) {
    let columns = columns.max(1);
    let row = index / columns;
    let col = index % columns;
    #[allow(clippy::cast_precision_loss)]
    (
        GRID_PADDING + col as f32 * CELL_WIDTH,
        row as f32 * ROW_HEIGHT,
        CELL_VISIBLE_WIDTH,
        CELL_VISIBLE_HEIGHT,
    )
}

/// Indices of the cells intersecting the rectangle spanned by two corners
/// (grid coordinates, any order), in index order.
#[must_use]
pub(crate) fn cells_in_rect(
    items: usize,
    columns: usize,
    a: (f32, f32),
    b: (f32, f32),
) -> Vec<usize> {
    let columns = columns.max(1);
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    if items == 0 || x1 < 0.0 || y1 < 0.0 {
        return Vec::new();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let first_row = (y0.max(0.0) / ROW_HEIGHT).floor() as usize;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let last_row = (y1.max(0.0) / ROW_HEIGHT).floor() as usize;
    let mut out = Vec::new();
    for row in first_row..=last_row.min(row_count(items, columns).saturating_sub(1)) {
        #[allow(clippy::cast_precision_loss)]
        let row_top = row as f32 * ROW_HEIGHT;
        if y0 > row_top + CELL_VISIBLE_HEIGHT || y1 < row_top {
            continue;
        }
        for col in 0..columns {
            let index = row * columns + col;
            if index >= items {
                break;
            }
            let (cx, _, cw, _) = cell_rect(index, columns);
            if x1 >= cx && x0 <= cx + cw {
                out.push(index);
            }
        }
    }
    out
}

/// A keyboard move inside the grid (Fluent GridView inner navigation).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum GridNav {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

/// Where focus lands after `nav` from `from` in a grid of `len` items laid
/// out in `columns`, with `page_rows` fully visible rows. `None` for an
/// empty grid. A stale `from` is clamped first; moving down from the last
/// row stays put, and down into a shorter last row lands on the last item.
#[must_use]
pub(crate) fn grid_nav(
    from: usize,
    len: usize,
    columns: usize,
    page_rows: usize,
    nav: GridNav,
) -> Option<usize> {
    let last = len.checked_sub(1)?;
    let cols = columns.max(1);
    let from = from.min(last);
    let down = |rows: usize| {
        if row_of(from, cols) == row_of(last, cols) {
            from
        } else {
            (from + rows * cols).min(last)
        }
    };
    let up = |rows: usize| from.checked_sub(rows * cols).unwrap_or(from % cols);
    let page = page_rows.max(1);
    Some(match nav {
        GridNav::Left => from.saturating_sub(1),
        GridNav::Right => (from + 1).min(last),
        GridNav::Up => up(1),
        GridNav::Down => down(1),
        GridNav::Home => 0,
        GridNav::End => last,
        GridNav::PageUp => up(page),
        GridNav::PageDown => down(page),
    })
}

/// How a keyboard move affects the selection (Fluent extended selection).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeyMode {
    /// Plain arrow: selection follows focus.
    Replace,
    /// Shift: range from the anchor to the new focus.
    Extend,
    /// Ctrl/Cmd: focus moves, selection stays.
    FocusOnly,
}

/// Multi-selection over the ordered grid items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridSelection {
    pub ids: std::collections::HashSet<clipforge_core::MediaId>,
    /// Anchor for shift-range selection, as an index into the current order.
    pub anchor: Option<usize>,
    /// Keyboard focus, as an index into the current order. Clicks move it
    /// too, so the keyboard continues from where the mouse was.
    pub focus: Option<usize>,
}

impl GridSelection {
    /// Click semantics: plain replaces, shift extends from the anchor,
    /// toggle (Cmd/Ctrl) flips one item.
    pub(crate) fn click(
        &mut self,
        ids: &[clipforge_core::MediaId],
        index: usize,
        shift: bool,
        toggle: bool,
    ) {
        let Some(&clicked) = ids.get(index) else {
            return;
        };
        self.focus = Some(index);
        if shift {
            let anchor = self.anchor.unwrap_or(index).min(ids.len() - 1);
            let (lo, hi) = if anchor <= index {
                (anchor, index)
            } else {
                (index, anchor)
            };
            if !toggle {
                self.ids.clear();
            }
            self.ids.extend(ids[lo..=hi].iter().copied());
        } else if toggle {
            if !self.ids.remove(&clicked) {
                self.ids.insert(clicked);
            }
            self.anchor = Some(index);
        } else {
            self.ids.clear();
            self.ids.insert(clicked);
            self.anchor = Some(index);
        }
    }

    /// Marquee result: replaces the selection, or adds to `base` when
    /// `additive` (shift/Cmd held when the drag started).
    pub(crate) fn marquee(
        &mut self,
        ids: &[clipforge_core::MediaId],
        hits: &[usize],
        base: &std::collections::HashSet<clipforge_core::MediaId>,
        additive: bool,
    ) {
        self.ids = if additive {
            base.clone()
        } else {
            std::collections::HashSet::new()
        };
        self.ids
            .extend(hits.iter().filter_map(|i| ids.get(*i).copied()));
    }

    /// Selected ids in grid order.
    #[must_use]
    pub(crate) fn ordered(&self, ids: &[clipforge_core::MediaId]) -> Vec<clipforge_core::MediaId> {
        ids.iter()
            .copied()
            .filter(|id| self.ids.contains(id))
            .collect()
    }

    /// Moves focus to `index` and updates the selection per `mode`.
    pub(crate) fn key_select(
        &mut self,
        ids: &[clipforge_core::MediaId],
        index: usize,
        mode: KeyMode,
    ) {
        if index >= ids.len() {
            return;
        }
        match mode {
            KeyMode::Replace => self.click(ids, index, false, false),
            KeyMode::Extend => self.click(ids, index, true, false),
            KeyMode::FocusOnly => self.focus = Some(index),
        }
    }

    /// Space: flips the focused item in or out of the selection.
    pub(crate) fn toggle_focused(&mut self, ids: &[clipforge_core::MediaId]) {
        if let Some(f) = self.focus {
            self.click(ids, f, false, true);
        }
    }

    /// Where focus goes when the grid is entered with Tab: the last focused
    /// item, else the first selected one, else the first item.
    #[must_use]
    pub(crate) fn entry_focus(&self, ids: &[clipforge_core::MediaId]) -> Option<usize> {
        if ids.is_empty() {
            return None;
        }
        self.focus
            .filter(|f| *f < ids.len())
            .or_else(|| ids.iter().position(|id| self.ids.contains(id)))
            .or(Some(0))
    }

    pub(crate) fn retain_existing(&mut self, ids: &[clipforge_core::MediaId]) {
        let existing: std::collections::HashSet<_> = ids.iter().copied().collect();
        self.ids.retain(|id| existing.contains(id));
        if self.anchor.is_some_and(|a| a >= ids.len()) {
            self.anchor = None;
        }
        self.focus = match ids.len().checked_sub(1) {
            None => None,
            Some(last) => self.focus.map(|f| f.min(last)),
        };
    }

    pub(crate) fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }
}

/// Remembers which grid cells were shown recently so thumbnails for cells
/// far away can be dropped from memory.
#[derive(Debug, Default)]
pub(crate) struct ThumbWindow {
    order: VecDeque<usize>,
    capacity: usize,
}

impl ThumbWindow {
    #[must_use]
    pub(crate) fn new(capacity: usize) -> Self {
        ThumbWindow {
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// Records that `index` is visible. Returns indices whose images should
    /// be released now.
    pub(crate) fn shown(&mut self, index: usize) -> Vec<usize> {
        if let Some(pos) = self.order.iter().position(|i| *i == index) {
            self.order.remove(pos);
        }
        self.order.push_back(index);
        let mut evicted = Vec::new();
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                evicted.push(old);
            }
        }
        evicted
    }

    pub(crate) fn clear(&mut self) {
        self.order.clear();
    }

    #[must_use]
    pub(crate) fn contains(&self, index: usize) -> bool {
        self.order.contains(&index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_never_below_one_and_grow_with_width() {
        assert_eq!(columns_for_width(0.0), 1);
        assert_eq!(columns_for_width(200.0), 1);
        assert_eq!(columns_for_width(2.0 * GRID_PADDING + 3.0 * CELL_WIDTH), 3);
        assert_eq!(
            columns_for_width(2.0 * GRID_PADDING + 3.0 * CELL_WIDTH - 1.0),
            2
        );
    }

    #[test]
    fn rows_and_positions() {
        assert_eq!(row_count(0, 4), 0);
        assert_eq!(row_count(1, 4), 1);
        assert_eq!(row_count(4, 4), 1);
        assert_eq!(row_count(5, 4), 2);
        assert_eq!(row_count(5, 0), 5, "zero columns behaves like one");
        assert_eq!(row_of(7, 4), 1);
        assert_eq!(row_of(8, 4), 2);
    }

    #[test]
    fn view_state_builds_expected_query() {
        let q = ViewState::default().query();
        assert_eq!(q.kinds, None);
        assert_eq!(q.text, None);
        assert_eq!(q.sort, Sort::CapturedAt);
        assert!(q.descending);

        let q = ViewState {
            search: " beach ".into(),
            filter: KindFilter::from_index(2),
            sort: SortChoice::from_index(1),
            descending: SortChoice::from_index(1).default_descending(),
        }
        .query();
        assert_eq!(q.kinds, Some(vec![MediaKind::Video]));
        assert_eq!(q.text.as_deref(), Some(" beach "));
        assert_eq!(q.sort, Sort::Name);
        assert!(!q.descending);
        assert_eq!(KindFilter::from_index(99), KindFilter::All);
        assert_eq!(SortChoice::from_index(-1), SortChoice::Date);
    }

    #[test]
    fn cell_rects_and_marquee_hits() {
        let (x, y, w, h) = cell_rect(5, 3);
        assert_eq!(
            (x, y, w, h),
            (
                GRID_PADDING + 2.0 * CELL_WIDTH,
                ROW_HEIGHT,
                CELL_VISIBLE_WIDTH,
                CELL_VISIBLE_HEIGHT
            )
        );
        // A rectangle covering the middle of row 0 and row 1 in a 3-column grid of 7 items.
        let hits = cells_in_rect(
            7,
            3,
            (GRID_PADDING + CELL_WIDTH + 10.0, 10.0),
            (GRID_PADDING + 2.0 * CELL_WIDTH + 10.0, ROW_HEIGHT + 10.0),
        );
        assert_eq!(hits, [1, 2, 4, 5]);
        // Corners given in the other order give the same result.
        let same = cells_in_rect(
            7,
            3,
            (GRID_PADDING + 2.0 * CELL_WIDTH + 10.0, ROW_HEIGHT + 10.0),
            (GRID_PADDING + CELL_WIDTH + 10.0, 10.0),
        );
        assert_eq!(same, hits);
        // A rectangle in the gap between cells hits nothing.
        assert!(
            cells_in_rect(
                7,
                3,
                (GRID_PADDING + CELL_VISIBLE_WIDTH + 1.0, 5.0),
                (GRID_PADDING + CELL_WIDTH - 1.0, 6.0)
            )
            .is_empty()
        );
        // Beyond the last row is clamped; last partial row respected.
        assert_eq!(
            cells_in_rect(7, 3, (0.0, 2.0 * ROW_HEIGHT), (10_000.0, 10_000.0)),
            [6]
        );
        assert!(cells_in_rect(0, 3, (0.0, 0.0), (100.0, 100.0)).is_empty());
        assert!(cells_in_rect(7, 3, (-50.0, -50.0), (-1.0, -1.0)).is_empty());
    }

    #[test]
    fn grid_selection_semantics() {
        use clipforge_core::MediaId;
        let ids: Vec<MediaId> = (0..6).map(|_| MediaId::new()).collect();
        let mut s = GridSelection::default();
        s.click(&ids, 1, false, false);
        assert_eq!(s.ordered(&ids), [ids[1]]);
        s.click(&ids, 4, true, false);
        assert_eq!(s.ordered(&ids), ids[1..=4]);
        s.click(&ids, 0, false, true);
        assert_eq!(s.ordered(&ids).len(), 5);
        s.click(&ids, 2, false, true);
        assert!(!s.ids.contains(&ids[2]));
        s.click(&ids, 5, true, true);
        assert!(
            s.ids.contains(&ids[3]) && s.ids.contains(&ids[5]),
            "shift+toggle adds a range"
        );
        let base = s.ids.clone();
        s.marquee(&ids, &[0, 1], &base, false);
        assert_eq!(s.ordered(&ids), ids[0..=1]);
        s.marquee(&ids, &[5], &base, true);
        assert!(s.ids.contains(&ids[5]) && s.ids.contains(&ids[0]));
        s.retain_existing(&ids[..2]);
        assert_eq!(s.ordered(&ids), [ids[0], ids[1]]);
        s.click(&ids, 99, false, false);
        s.clear();
        assert!(s.ids.is_empty());
    }

    #[test]
    fn thumb_window_evicts_least_recently_shown() {
        let mut w = ThumbWindow::new(3);
        assert!(w.shown(1).is_empty());
        assert!(w.shown(2).is_empty());
        assert!(w.shown(3).is_empty());
        assert_eq!(
            w.shown(1),
            Vec::<usize>::new(),
            "re-showing moves to the back"
        );
        assert_eq!(w.shown(4), vec![2]);
        assert_eq!(w.shown(5), vec![3]);
        assert!(w.contains(1) && w.contains(4) && w.contains(5));
        w.clear();
        assert!(!w.contains(1));
    }

    #[test]
    fn grid_nav_moves_in_reading_order_and_clamps() {
        // 10 items in 4 columns: rows [0..4), [4..8), [8..10).
        let nav = |from, n| grid_nav(from, 10, 4, 2, n);
        assert_eq!(nav(0, GridNav::Left), Some(0), "clamps at the start");
        assert_eq!(nav(3, GridNav::Right), Some(4), "wraps to the next row");
        assert_eq!(nav(4, GridNav::Left), Some(3), "wraps to the previous row");
        assert_eq!(nav(9, GridNav::Right), Some(9), "clamps at the end");
        assert_eq!(nav(5, GridNav::Up), Some(1));
        assert_eq!(nav(1, GridNav::Up), Some(1), "top row stays");
        assert_eq!(nav(1, GridNav::Down), Some(5));
        assert_eq!(nav(6, GridNav::Home), Some(0));
        assert_eq!(nav(2, GridNav::End), Some(9));
        assert_eq!(grid_nav(0, 0, 4, 2, GridNav::Right), None, "empty grid");
    }

    #[test]
    fn grid_nav_down_into_short_last_row_lands_on_last_item() {
        // Column 3 of row 1 (index 7) has no cell below it in the last row.
        assert_eq!(grid_nav(7, 10, 4, 2, GridNav::Down), Some(9));
        assert_eq!(
            grid_nav(9, 10, 4, 2, GridNav::Down),
            Some(9),
            "last row stays"
        );
        assert_eq!(
            grid_nav(20, 10, 4, 2, GridNav::Left),
            Some(8),
            "stale index clamps, then moves"
        );
    }

    #[test]
    fn grid_nav_page_moves_by_visible_rows() {
        // 40 items, 4 columns, 3 visible rows.
        assert_eq!(grid_nav(1, 40, 4, 3, GridNav::PageDown), Some(13));
        assert_eq!(grid_nav(13, 40, 4, 3, GridNav::PageUp), Some(1));
        assert_eq!(
            grid_nav(5, 40, 4, 3, GridNav::PageUp),
            Some(1),
            "clamps to the top row"
        );
        assert_eq!(
            grid_nav(38, 40, 4, 3, GridNav::PageDown),
            Some(38),
            "last row stays"
        );
        assert_eq!(
            grid_nav(30, 40, 4, 3, GridNav::PageDown),
            Some(39),
            "short of a full page lands on the last item's row"
        );
        assert_eq!(
            grid_nav(1, 40, 4, 0, GridNav::PageDown),
            Some(5),
            "at least one row"
        );
    }

    fn media_ids(n: usize) -> Vec<clipforge_core::MediaId> {
        (0..n).map(|_| clipforge_core::MediaId::new()).collect()
    }

    #[test]
    fn keyboard_select_plain_replaces_and_sets_anchor() {
        let ids = media_ids(6);
        let mut sel = GridSelection::default();
        sel.key_select(&ids, 2, KeyMode::Replace);
        sel.key_select(&ids, 3, KeyMode::Replace);
        assert_eq!(sel.ordered(&ids), vec![ids[3]]);
        assert_eq!(sel.anchor, Some(3));
        assert_eq!(sel.focus, Some(3));
    }

    #[test]
    fn keyboard_select_shift_extends_from_anchor() {
        let ids = media_ids(6);
        let mut sel = GridSelection::default();
        sel.key_select(&ids, 1, KeyMode::Replace);
        sel.key_select(&ids, 2, KeyMode::Extend);
        sel.key_select(&ids, 4, KeyMode::Extend);
        assert_eq!(sel.ordered(&ids), ids[1..=4].to_vec());
        assert_eq!(sel.anchor, Some(1), "anchor stays while extending");
        sel.key_select(&ids, 0, KeyMode::Extend);
        assert_eq!(
            sel.ordered(&ids),
            ids[0..=1].to_vec(),
            "range flips around the anchor"
        );
    }

    #[test]
    fn keyboard_focus_only_leaves_the_selection() {
        let ids = media_ids(6);
        let mut sel = GridSelection::default();
        sel.key_select(&ids, 1, KeyMode::Replace);
        sel.key_select(&ids, 4, KeyMode::FocusOnly);
        assert_eq!(sel.ordered(&ids), vec![ids[1]]);
        assert_eq!(sel.focus, Some(4));
        assert_eq!(sel.anchor, Some(1));
    }

    #[test]
    fn keyboard_toggle_flips_one_and_moves_anchor() {
        let ids = media_ids(6);
        let mut sel = GridSelection::default();
        sel.key_select(&ids, 1, KeyMode::Replace);
        sel.key_select(&ids, 3, KeyMode::FocusOnly);
        sel.toggle_focused(&ids);
        assert_eq!(sel.ordered(&ids), vec![ids[1], ids[3]]);
        assert_eq!(sel.anchor, Some(3));
        sel.toggle_focused(&ids);
        assert_eq!(sel.ordered(&ids), vec![ids[1]]);
    }

    #[test]
    fn entry_focus_prefers_last_focus_then_selection_then_first() {
        let ids = media_ids(6);
        let mut sel = GridSelection::default();
        assert_eq!(sel.entry_focus(&ids), Some(0));
        sel.click(&ids, 4, false, false);
        sel.focus = None;
        assert_eq!(sel.entry_focus(&ids), Some(4));
        sel.focus = Some(2);
        assert_eq!(sel.entry_focus(&ids), Some(2));
        assert_eq!(GridSelection::default().entry_focus(&[]), None);
    }

    #[test]
    fn clicks_move_focus_and_refresh_clamps_it() {
        let ids = media_ids(6);
        let mut sel = GridSelection::default();
        sel.click(&ids, 5, false, false);
        assert_eq!(sel.focus, Some(5));
        sel.retain_existing(&ids[..3]);
        assert_eq!(sel.focus, Some(2));
        sel.retain_existing(&[]);
        assert_eq!(sel.focus, None);
    }
}
