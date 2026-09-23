//! Pure view logic for the library grid: layout maths, query state and the
//! window of rows whose thumbnails we keep in memory. No Slint here.

use std::collections::VecDeque;

use clipforge_library::{Query, Sort};
use clipforge_media::MediaKind;

/// Width of one grid cell including its gap, in logical pixels.
pub(crate) const CELL_WIDTH: f32 = 112.0;
/// Horizontal padding of the grid, in logical pixels.
pub(crate) const GRID_PADDING: f32 = 12.0;

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
}
