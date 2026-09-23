//! Background job primitives.
//!
//! Every slow operation in ClipForge (probing, thumbnails, proxies, export)
//! is a job: it has a [`Priority`], observes a [`CancellationToken`] and
//! reports [`Progress`]. The [`Scheduler`] runs jobs on a pool of worker
//! threads, highest priority first, and lets the owner re-prioritise or
//! cancel jobs that have not started yet (thumbnails for cells that scrolled
//! out of view, for example).

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod scheduler;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use scheduler::{JobContext, JobError, JobEvent, JobHandle, JobId, JobOutcome, Scheduler};

/// Scheduling priority. Lower variants run first.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// The user is waiting for this right now (visible thumbnail, scrub frame).
    Interactive,
    /// Needed very soon (items just outside the viewport, next clips).
    Soon,
    /// Regular background work (probing, full-size thumbnails).
    Background,
    /// Only when nothing else is pending (proxies, cache maintenance).
    Idle,
}

/// Cooperative cancellation flag shared between a job and its owner.
///
/// Cloning yields a handle to the same flag. A child token is cancelled when
/// its parent is, but not the other way round.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    inner: Arc<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    cancelled: AtomicBool,
    parent: Option<CancellationToken>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a token that is cancelled whenever `self` is.
    #[must_use]
    pub fn child(&self) -> Self {
        CancellationToken {
            inner: Arc::new(Inner {
                cancelled: AtomicBool::new(false),
                parent: Some(self.clone()),
            }),
        }
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
            || self
                .inner
                .parent
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
    }

    /// Returns `Err(Cancelled)` if cancellation was requested. Handy in loops:
    /// `token.check()?;`
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Error returned when a job observed cancellation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("operation cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// A progress snapshot. `total` is `None` while the amount of work is unknown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
    pub message: String,
}

impl Progress {
    #[must_use]
    pub fn indeterminate(message: impl Into<String>) -> Self {
        Progress {
            done: 0,
            total: None,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn of(done: u64, total: u64, message: impl Into<String>) -> Self {
        Progress {
            done,
            total: Some(total),
            message: message.into(),
        }
    }

    /// Completion in `0.0..=1.0`, or `None` when indeterminate.
    #[must_use]
    pub fn fraction(&self) -> Option<f64> {
        match self.total {
            Some(0) => Some(1.0),
            #[allow(clippy::cast_precision_loss)]
            Some(total) => Some((self.done.min(total) as f64) / (total as f64)),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priorities_order_interactive_first() {
        let mut v = vec![
            Priority::Idle,
            Priority::Background,
            Priority::Interactive,
            Priority::Soon,
        ];
        v.sort();
        assert_eq!(
            v,
            [
                Priority::Interactive,
                Priority::Soon,
                Priority::Background,
                Priority::Idle
            ]
        );
    }

    #[test]
    fn clones_share_the_flag() {
        let a = CancellationToken::new();
        let b = a.clone();
        assert!(!b.is_cancelled());
        a.cancel();
        assert!(b.is_cancelled());
        assert_eq!(b.check(), Err(Cancelled));
    }

    #[test]
    fn child_follows_parent_but_not_vice_versa() {
        let parent = CancellationToken::new();
        let child = parent.child();
        child.cancel();
        assert!(child.is_cancelled());
        assert!(!parent.is_cancelled());

        let parent = CancellationToken::new();
        let child = parent.child();
        parent.cancel();
        assert!(child.is_cancelled());
    }

    #[test]
    fn progress_fraction() {
        assert_eq!(Progress::indeterminate("x").fraction(), None);
        assert_eq!(Progress::of(1, 4, "x").fraction(), Some(0.25));
        assert_eq!(Progress::of(9, 4, "x").fraction(), Some(1.0));
        assert_eq!(Progress::of(0, 0, "x").fraction(), Some(1.0));
    }
}
