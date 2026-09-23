//! Filtering, searching and sorting the catalogue.

use clipforge_media::MediaKind;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    /// Capture time, falling back to file modification time.
    #[default]
    CapturedAt,
    Name,
    Kind,
    AddedAt,
    Duration,
    FileSize,
}

/// A catalogue query. `Default` returns everything, newest capture first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// Restrict to these kinds; `None` means all.
    pub kinds: Option<Vec<MediaKind>>,
    /// Case-insensitive substring search over file name and folder.
    pub text: Option<String>,
    /// Inclusive range on the sort time in Unix milliseconds.
    pub time_range_ms: Option<(i64, i64)>,
    pub only_hdr: bool,
    pub only_failed: bool,
    pub only_placeholders: bool,
    pub sort: Sort,
    pub descending: bool,
    pub offset: u64,
    pub limit: Option<u64>,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            kinds: None,
            text: None,
            time_range_ms: None,
            only_hdr: false,
            only_failed: false,
            only_placeholders: false,
            sort: Sort::CapturedAt,
            descending: true,
            offset: 0,
            limit: None,
        }
    }
}

impl Query {
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn kinds(mut self, kinds: impl IntoIterator<Item = MediaKind>) -> Self {
        self.kinds = Some(kinds.into_iter().collect());
        self
    }

    #[must_use]
    pub fn text(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.text = if text.trim().is_empty() {
            None
        } else {
            Some(text)
        };
        self
    }

    #[must_use]
    pub fn sort(mut self, sort: Sort, descending: bool) -> Self {
        self.sort = sort;
        self.descending = descending;
        self
    }

    #[must_use]
    pub fn page(mut self, offset: u64, limit: u64) -> Self {
        self.offset = offset;
        self.limit = Some(limit);
        self
    }
}
