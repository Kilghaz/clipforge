//! A small priority scheduler on top of plain threads.
//!
//! Design goals: no async runtime, deterministic tests (a scheduler with a
//! manual `tick` step), cheap re-prioritisation of queued jobs, and a single
//! event stream the UI can drain on its own thread.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, atomic::AtomicU64, atomic::Ordering};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{CancellationToken, Cancelled, Priority, Progress};

/// Identifies a submitted job.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

/// Why a job did not complete normally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobError {
    Cancelled,
    /// The job body returned an error message meant for logs and the UI.
    Failed(String),
    /// The job body panicked. The message is what could be extracted.
    Panicked(String),
}

impl From<Cancelled> for JobError {
    fn from(_: Cancelled) -> Self {
        JobError::Cancelled
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobError::Cancelled => f.write_str("cancelled"),
            JobError::Failed(m) => write!(f, "failed: {m}"),
            JobError::Panicked(m) => write!(f, "panicked: {m}"),
        }
    }
}

impl std::error::Error for JobError {}

pub type JobOutcome = Result<(), JobError>;

/// What a job body receives.
#[derive(Clone, Debug)]
pub struct JobContext {
    pub id: JobId,
    pub token: CancellationToken,
    events: Sender<JobEvent>,
}

impl JobContext {
    /// Reports progress to whoever drains [`Scheduler::events`].
    pub fn progress(&self, progress: Progress) {
        let _ = self.events.send(JobEvent::Progress {
            id: self.id,
            progress,
        });
    }

    /// Shorthand for `self.token.check()`.
    pub fn check(&self) -> Result<(), Cancelled> {
        self.token.check()
    }
}

/// Lifecycle notifications, in order per job: `Started`, any number of
/// `Progress`, then exactly one `Finished`. Cancelled-before-start jobs emit
/// only `Finished(Err(Cancelled))`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobEvent {
    Started { id: JobId },
    Progress { id: JobId, progress: Progress },
    Finished { id: JobId, outcome: JobOutcome },
}

impl JobEvent {
    #[must_use]
    pub fn id(&self) -> JobId {
        match self {
            JobEvent::Started { id }
            | JobEvent::Progress { id, .. }
            | JobEvent::Finished { id, .. } => *id,
        }
    }
}

/// Handle returned on submission.
#[derive(Clone, Debug)]
pub struct JobHandle {
    pub id: JobId,
    pub token: CancellationToken,
}

impl JobHandle {
    pub fn cancel(&self) {
        self.token.cancel();
    }
}

type JobBody = Box<dyn FnOnce(&JobContext) -> JobOutcome + Send + 'static>;

struct Queued {
    body: JobBody,
    token: CancellationToken,
    label: &'static str,
}

/// Queue key: lower priority value first, then submission order.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    priority: Priority,
    seq: u64,
}

#[derive(Default)]
struct Queue {
    by_key: BTreeMap<Key, Queued>,
    key_of: BTreeMap<JobId, Key>,
    shutting_down: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: crossbeam_channel::Sender<()>,
    events: Sender<JobEvent>,
    next_seq: AtomicU64,
}

/// Runs jobs on worker threads by priority.
pub struct Scheduler {
    shared: Arc<Shared>,
    events_rx: Receiver<JobEvent>,
    wake_rx: Receiver<()>,
    workers: Vec<JoinHandle<()>>,
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scheduler")
            .field("workers", &self.workers.len())
            .field("queued", &self.queued())
            .finish()
    }
}

impl Scheduler {
    /// Starts `workers` threads. Zero workers gives a manual scheduler for
    /// tests: nothing runs until [`Scheduler::run_one`] is called.
    #[must_use]
    pub fn new(workers: usize) -> Self {
        let (events_tx, events_rx) = unbounded();
        let (wake_tx, wake_rx) = unbounded();
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: wake_tx,
            events: events_tx,
            next_seq: AtomicU64::new(1),
        });
        let workers = (0..workers)
            .map(|i| {
                let shared = Arc::clone(&shared);
                let wake_rx = wake_rx.clone();
                std::thread::Builder::new()
                    .name(format!("clipforge-job-{i}"))
                    .spawn(move || worker_loop(&shared, &wake_rx))
                    .unwrap_or_else(|e| {
                        // Thread creation only fails on resource exhaustion; a
                        // scheduler with fewer workers is still correct.
                        tracing::error!(error = %e, "could not spawn job worker");
                        std::thread::spawn(|| {})
                    })
            })
            .collect();
        Scheduler {
            shared,
            events_rx,
            wake_rx,
            workers,
        }
    }

    /// Queues a job. `label` shows up in logs and thread names.
    pub fn submit<F>(&self, priority: Priority, label: &'static str, body: F) -> JobHandle
    where
        F: FnOnce(&JobContext) -> JobOutcome + Send + 'static,
    {
        let seq = self.shared.next_seq.fetch_add(1, Ordering::Relaxed);
        let id = JobId(seq);
        let token = CancellationToken::new();
        {
            let mut q = lock(&self.shared.queue);
            let key = Key { priority, seq };
            q.by_key.insert(
                key,
                Queued {
                    body: Box::new(body),
                    token: token.clone(),
                    label,
                },
            );
            q.key_of.insert(id, key);
        }
        let _ = self.shared.wake.send(());
        JobHandle { id, token }
    }

    /// Changes the priority of a job that has not started yet. Returns
    /// `false` if the job is running or gone.
    pub fn reprioritize(&self, id: JobId, priority: Priority) -> bool {
        let mut q = lock(&self.shared.queue);
        let Some(old_key) = q.key_of.get(&id).copied() else {
            return false;
        };
        if old_key.priority == priority {
            return true;
        }
        let Some(job) = q.by_key.remove(&old_key) else {
            return false;
        };
        let new_key = Key {
            priority,
            seq: old_key.seq,
        };
        q.by_key.insert(new_key, job);
        q.key_of.insert(id, new_key);
        true
    }

    /// Cancels a job. Queued jobs are dropped immediately and emit
    /// `Finished(Err(Cancelled))`; running jobs see their token flip.
    pub fn cancel(&self, id: JobId) {
        let removed = {
            let mut q = lock(&self.shared.queue);
            match q.key_of.remove(&id) {
                Some(key) => q.by_key.remove(&key),
                None => None,
            }
        };
        if let Some(job) = removed {
            job.token.cancel();
            let _ = self.shared.events.send(JobEvent::Finished {
                id,
                outcome: Err(JobError::Cancelled),
            });
        }
        // Running job: the handle's token is shared; cancel through it is the
        // caller's responsibility (JobHandle::cancel). Nothing else to do.
    }

    /// Cancels every queued job (running jobs finish on their own).
    pub fn cancel_all_queued(&self) {
        let drained: Vec<(JobId, Queued)> = {
            let mut q = lock(&self.shared.queue);
            let by_key = std::mem::take(&mut q.by_key);
            q.key_of.clear();
            by_key
                .into_iter()
                .map(|(k, job)| (JobId(k.seq), job))
                .collect()
        };
        for (id, job) in drained {
            job.token.cancel();
            let _ = self.shared.events.send(JobEvent::Finished {
                id,
                outcome: Err(JobError::Cancelled),
            });
        }
    }

    /// Number of jobs waiting to start.
    #[must_use]
    pub fn queued(&self) -> usize {
        lock(&self.shared.queue).by_key.len()
    }

    /// Event stream. Drain it regularly from the UI thread.
    #[must_use]
    pub fn events(&self) -> &Receiver<JobEvent> {
        &self.events_rx
    }

    /// Runs the highest-priority queued job on the calling thread. Returns
    /// `false` if the queue was empty. Meant for tests and for zero-worker
    /// schedulers.
    pub fn run_one(&self) -> bool {
        run_next(&self.shared)
    }

    /// Manual schedulers only: the wake receiver, so a test can wait for a
    /// submission without polling.
    #[must_use]
    pub fn wake_signal(&self) -> &Receiver<()> {
        &self.wake_rx
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        {
            let mut q = lock(&self.shared.queue);
            q.shutting_down = true;
            q.by_key.clear();
            q.key_of.clear();
        }
        for _ in &self.workers {
            let _ = self.shared.wake.send(());
        }
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

fn worker_loop(shared: &Arc<Shared>, wake: &Receiver<()>) {
    loop {
        if lock(&shared.queue).shutting_down {
            return;
        }
        if !run_next(shared) {
            // Nothing to do: block until a submission or shutdown wakes us.
            if wake.recv().is_err() {
                return;
            }
        }
    }
}

fn run_next(shared: &Arc<Shared>) -> bool {
    let (id, job) = {
        let mut q = lock(&shared.queue);
        let Some((key, _)) = q.by_key.first_key_value() else {
            return false;
        };
        let key = *key;
        let Some(job) = q.by_key.remove(&key) else {
            return false;
        };
        let id = JobId(key.seq);
        q.key_of.remove(&id);
        (id, job)
    };
    if job.token.is_cancelled() {
        let _ = shared.events.send(JobEvent::Finished {
            id,
            outcome: Err(JobError::Cancelled),
        });
        return true;
    }
    let ctx = JobContext {
        id,
        token: job.token.clone(),
        events: shared.events.clone(),
    };
    let _ = shared.events.send(JobEvent::Started { id });
    let span = tracing::debug_span!("job", label = job.label, id = id.0);
    let _enter = span.enter();
    let body = job.body;
    let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || body(&ctx))) {
        Ok(outcome) => outcome,
        Err(payload) => Err(JobError::Panicked(panic_message(payload.as_ref()))),
    };
    if let Err(e) = &outcome {
        tracing::debug!(label = job.label, error = %e, "job did not complete");
    }
    let _ = shared.events.send(JobEvent::Finished { id, outcome });
    true
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A poisoned queue only means a panic while holding the lock; the data
    // is still a consistent map, so continue rather than take the app down.
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    fn drain(s: &Scheduler) -> Vec<JobEvent> {
        s.events().try_iter().collect()
    }

    #[test]
    fn manual_scheduler_runs_by_priority_then_fifo() {
        let s = Scheduler::new(0);
        let order = Arc::new(Mutex::new(Vec::new()));
        let push = |prio, tag: &'static str| {
            let order = Arc::clone(&order);
            s.submit(prio, tag, move |_| {
                lock(&order).push(tag);
                Ok(())
            })
        };
        push(Priority::Idle, "idle");
        push(Priority::Background, "bg1");
        push(Priority::Interactive, "hot");
        push(Priority::Background, "bg2");
        assert_eq!(s.queued(), 4);
        while s.run_one() {}
        assert_eq!(*lock(&order), ["hot", "bg1", "bg2", "idle"]);
        assert!(!s.run_one());
    }

    #[test]
    fn events_follow_started_progress_finished() {
        let s = Scheduler::new(0);
        let h = s.submit(Priority::Soon, "p", |ctx| {
            ctx.progress(Progress::of(1, 2, "half"));
            Ok(())
        });
        s.run_one();
        let ev = drain(&s);
        assert_eq!(
            ev,
            [
                JobEvent::Started { id: h.id },
                JobEvent::Progress {
                    id: h.id,
                    progress: Progress::of(1, 2, "half")
                },
                JobEvent::Finished {
                    id: h.id,
                    outcome: Ok(())
                },
            ]
        );
    }

    #[test]
    fn cancelling_a_queued_job_drops_it_and_reports() {
        let s = Scheduler::new(0);
        let ran = Arc::new(AtomicUsize::new(0));
        let ran2 = Arc::clone(&ran);
        let h = s.submit(Priority::Background, "c", move |_| {
            ran2.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        s.cancel(h.id);
        assert_eq!(s.queued(), 0);
        assert!(!s.run_one());
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        assert_eq!(
            drain(&s),
            [JobEvent::Finished {
                id: h.id,
                outcome: Err(JobError::Cancelled)
            }]
        );
        assert!(h.token.is_cancelled());
    }

    #[test]
    fn handle_cancel_before_start_skips_body() {
        let s = Scheduler::new(0);
        let h = s.submit(Priority::Background, "c", |_| panic!("must not run"));
        h.cancel();
        assert!(s.run_one());
        assert_eq!(
            drain(&s),
            [JobEvent::Finished {
                id: h.id,
                outcome: Err(JobError::Cancelled)
            }]
        );
    }

    #[test]
    fn running_job_observes_cancellation_through_context() {
        let s = Scheduler::new(0);
        let h = s.submit(Priority::Background, "loop", |ctx| {
            for _ in 0..1000 {
                ctx.check()?;
                ctx.token.cancel(); // simulate the owner cancelling mid-way
            }
            Ok(())
        });
        s.run_one();
        let last = drain(&s).pop().unwrap();
        assert_eq!(
            last,
            JobEvent::Finished {
                id: h.id,
                outcome: Err(JobError::Cancelled)
            }
        );
    }

    #[test]
    fn reprioritize_moves_queued_jobs() {
        let s = Scheduler::new(0);
        let order = Arc::new(Mutex::new(Vec::new()));
        let o1 = Arc::clone(&order);
        let o2 = Arc::clone(&order);
        let a = s.submit(Priority::Idle, "a", move |_| {
            lock(&o1).push("a");
            Ok(())
        });
        let _b = s.submit(Priority::Background, "b", move |_| {
            lock(&o2).push("b");
            Ok(())
        });
        assert!(s.reprioritize(a.id, Priority::Interactive));
        assert!(
            s.reprioritize(a.id, Priority::Interactive),
            "same priority is a no-op success"
        );
        while s.run_one() {}
        assert_eq!(*lock(&order), ["a", "b"]);
        assert!(
            !s.reprioritize(a.id, Priority::Idle),
            "finished jobs cannot be moved"
        );
    }

    #[test]
    fn panics_are_contained_and_reported() {
        let s = Scheduler::new(0);
        let h = s.submit(Priority::Background, "boom", |_| panic!("kaboom"));
        assert!(s.run_one());
        let last = drain(&s).pop().unwrap();
        assert_eq!(
            last,
            JobEvent::Finished {
                id: h.id,
                outcome: Err(JobError::Panicked("kaboom".into()))
            }
        );
        // Scheduler still works afterwards.
        let h2 = s.submit(Priority::Background, "ok", |_| Ok(()));
        s.run_one();
        assert_eq!(
            drain(&s).pop().unwrap(),
            JobEvent::Finished {
                id: h2.id,
                outcome: Ok(())
            }
        );
    }

    #[test]
    fn failures_carry_their_message() {
        let s = Scheduler::new(0);
        let h = s.submit(Priority::Background, "f", |_| {
            Err(JobError::Failed("disk full".into()))
        });
        s.run_one();
        assert_eq!(
            drain(&s).pop().unwrap(),
            JobEvent::Finished {
                id: h.id,
                outcome: Err(JobError::Failed("disk full".into()))
            }
        );
    }

    #[test]
    fn cancel_all_queued_reports_each() {
        let s = Scheduler::new(0);
        let ids: Vec<_> = (0..3)
            .map(|_| s.submit(Priority::Idle, "x", |_| Ok(())).id)
            .collect();
        s.cancel_all_queued();
        assert_eq!(s.queued(), 0);
        let mut finished: Vec<_> = drain(&s).into_iter().map(|e| e.id()).collect();
        finished.sort();
        assert_eq!(finished, ids);
    }

    #[test]
    fn threaded_scheduler_runs_everything_and_shuts_down() {
        let s = Scheduler::new(3);
        let counter = Arc::new(AtomicUsize::new(0));
        let n = 50;
        for _ in 0..n {
            let c = Arc::clone(&counter);
            s.submit(Priority::Background, "inc", move |_| {
                std::thread::sleep(Duration::from_millis(1));
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut finished = 0;
        while finished < n && std::time::Instant::now() < deadline {
            if let Ok(ev) = s.events().recv_timeout(Duration::from_millis(100))
                && matches!(ev, JobEvent::Finished { .. })
            {
                finished += 1;
            }
        }
        assert_eq!(finished, n);
        assert_eq!(counter.load(Ordering::SeqCst), n);
        drop(s); // must not hang
    }

    #[test]
    fn dropping_with_queued_jobs_does_not_run_them() {
        let ran = Arc::new(AtomicUsize::new(0));
        {
            let s = Scheduler::new(0);
            let r = Arc::clone(&ran);
            s.submit(Priority::Idle, "late", move |_| {
                r.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
        }
        assert_eq!(ran.load(Ordering::SeqCst), 0);
    }
}
