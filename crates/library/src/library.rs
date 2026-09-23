//! The library facade: catalogue + cache + background jobs.
//!
//! The UI talks to [`Library`] only. Slow work (enumeration, fingerprints,
//! probing, thumbnails) runs as jobs on the shared [`Scheduler`]; results
//! arrive as [`LibraryEvent`]s on a channel the UI drains on its thread.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use clipforge_core::MediaId;
use clipforge_jobs::{
    JobContext, JobError, JobHandle, JobId, JobOutcome, Priority, Progress, Scheduler,
};
use clipforge_media::{Backends, Prober, StillDecoder};
use clipforge_platform::AppDirs;
use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::cache::ThumbCache;
use crate::catalogue::Catalogue;
use crate::error::Result as LibResult;
use crate::import;
use crate::record::{CloudState, ThumbLevel, ThumbRecord};

/// Items registered per catalogue transaction during import.
const IMPORT_BATCH: usize = 200;
/// Items probed per background job.
const PROBE_BATCH: usize = 25;

/// What the UI hears about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryEvent {
    ImportStarted {
        total: usize,
    },
    ImportProgress {
        done: usize,
        total: usize,
    },
    /// New rows exist in the catalogue (not probed yet).
    ItemsAdded(Vec<MediaId>),
    ImportFinished {
        added: usize,
        existing: usize,
        failed: usize,
    },
    /// Probe finished (successfully or not); re-read the record.
    ItemUpdated(MediaId),
    ThumbReady {
        id: MediaId,
        level: ThumbLevel,
        path: PathBuf,
    },
    ThumbFailed {
        id: MediaId,
        level: ThumbLevel,
        message: String,
    },
}

struct Shared {
    catalogue: Mutex<Catalogue>,
    cache: ThumbCache,
    backends: Backends,
    events: Sender<LibraryEvent>,
    thumb_jobs: Mutex<HashMap<(MediaId, ThumbLevel), JobId>>,
    /// Thumbnails that failed this session; not retried until the item
    /// changes (relink, re-probe) or the app restarts.
    failed_thumbs: Mutex<HashSet<(MediaId, ThumbLevel)>>,
}

/// Owns the catalogue and drives all library work.
pub struct Library {
    shared: Arc<Shared>,
    scheduler: Arc<Scheduler>,
    events_rx: Receiver<LibraryEvent>,
}

impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Library")
            .field("cache", &self.shared.cache.root())
            .finish()
    }
}

impl Library {
    /// Opens the catalogue under `dirs.data` and the cache under `dirs.cache`.
    pub fn open(
        dirs: &AppDirs,
        backends: Backends,
        scheduler: Arc<Scheduler>,
    ) -> LibResult<Library> {
        let catalogue = Catalogue::open(&dirs.data.join("library.sqlite"))?;
        let cache = ThumbCache::new(dirs.cache.join("thumbs"));
        Ok(Self::with_parts(catalogue, cache, backends, scheduler))
    }

    /// Assembles a library from parts (tests, previews).
    #[must_use]
    pub fn with_parts(
        catalogue: Catalogue,
        cache: ThumbCache,
        backends: Backends,
        scheduler: Arc<Scheduler>,
    ) -> Library {
        let (events, events_rx) = unbounded();
        let shared = Arc::new(Shared {
            catalogue: Mutex::new(catalogue),
            cache,
            backends,
            events,
            thumb_jobs: Mutex::new(HashMap::new()),
            failed_thumbs: Mutex::new(HashSet::new()),
        });
        Library {
            shared,
            scheduler,
            events_rx,
        }
    }

    /// Direct, synchronous access to the catalogue for queries. Keep the
    /// guard short-lived: jobs need the same lock.
    pub fn catalogue(&self) -> MutexGuard<'_, Catalogue> {
        lock(&self.shared.catalogue)
    }

    #[must_use]
    pub fn cache(&self) -> &ThumbCache {
        &self.shared.cache
    }

    #[must_use]
    pub fn events(&self) -> &Receiver<LibraryEvent> {
        &self.events_rx
    }

    #[must_use]
    pub fn scheduler(&self) -> &Arc<Scheduler> {
        &self.scheduler
    }

    /// Registers files and folders. Returns immediately; progress arrives as
    /// events. New items are probed afterwards in background jobs.
    pub fn import(&self, roots: Vec<PathBuf>) -> JobHandle {
        let shared = Arc::clone(&self.shared);
        let scheduler = Arc::clone(&self.scheduler);
        self.scheduler.submit(Priority::Soon, "import", move |ctx| {
            run_import(&shared, &scheduler, ctx, &roots)
        })
    }

    /// Asks for a thumbnail. If it is cached, its path is returned at once
    /// (no event). Otherwise a job is queued (or re-prioritised if already
    /// queued) and a `ThumbReady`/`ThumbFailed` event follows.
    pub fn request_thumb(
        &self,
        id: MediaId,
        level: ThumbLevel,
        priority: Priority,
    ) -> Option<PathBuf> {
        let record = lock(&self.shared.catalogue).get(id).ok()?;
        let path = self.shared.cache.path_for(record.fingerprint, level);
        if path.is_file() {
            return Some(path);
        }
        let key = (id, level);
        {
            let mut jobs = lock(&self.shared.thumb_jobs);
            if let Some(job_id) = jobs.get(&key) {
                if self.scheduler.reprioritize(*job_id, priority) {
                    return None;
                }
                jobs.remove(&key);
            }
            let shared = Arc::clone(&self.shared);
            let handle = self.scheduler.submit(priority, "thumbnail", move |ctx| {
                let outcome = make_thumb(&shared, ctx, id, level);
                lock(&shared.thumb_jobs).remove(&key);
                outcome
            });
            jobs.insert(key, handle.id);
        }
        None
    }

    /// Lowers the priority of a queued thumbnail job (cell scrolled away).
    pub fn demote_thumb(&self, id: MediaId, level: ThumbLevel) {
        if let Some(job_id) = lock(&self.shared.thumb_jobs).get(&(id, level)) {
            self.scheduler.reprioritize(*job_id, Priority::Idle);
        }
    }

    /// Removes items from the catalogue and their thumbnails from the cache.
    /// Files on disk are never touched.
    pub fn remove(&self, ids: &[MediaId]) -> LibResult<usize> {
        let mut cat = lock(&self.shared.catalogue);
        let fps: Vec<_> = ids
            .iter()
            .filter_map(|id| cat.get(*id).ok().map(|r| r.fingerprint))
            .collect();
        let removed = cat.remove(ids)?;
        drop(cat);
        for fp in fps {
            let _ = self.shared.cache.remove(fp);
        }
        Ok(removed)
    }
}

fn run_import(
    shared: &Arc<Shared>,
    scheduler: &Arc<Scheduler>,
    ctx: &JobContext,
    roots: &[PathBuf],
) -> JobOutcome {
    let candidates = import::enumerate(roots);
    let total = candidates.len();
    let _ = shared.events.send(LibraryEvent::ImportStarted { total });
    ctx.progress(Progress::of(0, total as u64, "Scanning"));

    let (mut added, mut existing, mut failed) = (0usize, 0usize, 0usize);
    let mut new_ids: Vec<MediaId> = Vec::new();
    let mut done = 0usize;
    for chunk in candidates.chunks(IMPORT_BATCH) {
        ctx.check()?;
        let mut rows = Vec::with_capacity(chunk.len());
        for c in chunk {
            ctx.check()?;
            match import::prepare(c) {
                Ok(row) => rows.push(row),
                Err(e) => {
                    tracing::debug!(path = %c.path.display(), error = %e, "cannot fingerprint");
                    failed += 1;
                }
            }
        }
        let outcomes = lock(&shared.catalogue)
            .add_many(&rows, now_ms())
            .map_err(|e| JobError::Failed(e.to_string()))?;
        let mut batch_new = Vec::new();
        for o in outcomes {
            if o.is_new() {
                added += 1;
                batch_new.push(o.record().id);
            } else {
                existing += 1;
            }
        }
        done += chunk.len();
        if !batch_new.is_empty() {
            let _ = shared
                .events
                .send(LibraryEvent::ItemsAdded(batch_new.clone()));
            new_ids.extend(batch_new);
        }
        let _ = shared
            .events
            .send(LibraryEvent::ImportProgress { done, total });
        ctx.progress(Progress::of(done as u64, total as u64, "Adding files"));
    }

    for ids in new_ids.chunks(PROBE_BATCH) {
        let ids = ids.to_vec();
        let shared = Arc::clone(shared);
        scheduler.submit(Priority::Background, "probe", move |ctx| {
            run_probe(&shared, ctx, &ids)
        });
    }
    let _ = shared.events.send(LibraryEvent::ImportFinished {
        added,
        existing,
        failed,
    });
    Ok(())
}

fn run_probe(shared: &Arc<Shared>, ctx: &JobContext, ids: &[MediaId]) -> JobOutcome {
    for (i, id) in ids.iter().enumerate() {
        ctx.check()?;
        ctx.progress(Progress::of(i as u64, ids.len() as u64, "Reading metadata"));
        let Ok(record) = lock(&shared.catalogue).get(*id) else {
            continue;
        };
        if record.cloud_state == CloudState::Placeholder {
            // Probing would download the file; leave it pending.
            continue;
        }
        let result = shared.backends.probe(&record.path);
        {
            let mut cat = lock(&shared.catalogue);
            let stored = match &result {
                Ok(info) => cat.set_info(*id, info),
                Err(e) => cat.set_probe_failed(*id, &e.to_string()),
            };
            if let Err(e) = stored {
                tracing::warn!(%id, error = %e, "could not store probe result");
            }
        }
        lock(&shared.failed_thumbs).retain(|(fid, _)| fid != id);
        let _ = shared.events.send(LibraryEvent::ItemUpdated(*id));
        if result.is_ok()
            && record.kind != clipforge_media::MediaKind::Audio
            && !shared.cache.exists(record.fingerprint, ThumbLevel::Small)
        {
            // Cheap eager thumbnail so the grid fills without round trips.
            let _ = make_thumb(shared, ctx, *id, ThumbLevel::Small);
        }
    }
    Ok(())
}

fn make_thumb(
    shared: &Arc<Shared>,
    ctx: &JobContext,
    id: MediaId,
    level: ThumbLevel,
) -> JobOutcome {
    ctx.check()?;
    let record = match lock(&shared.catalogue).get(id) {
        Ok(r) => r,
        Err(e) => return Err(JobError::Failed(e.to_string())),
    };
    let fail = |message: String| {
        let _ = shared.events.send(LibraryEvent::ThumbFailed {
            id,
            level,
            message: message.clone(),
        });
        Err(JobError::Failed(message))
    };
    if record.cloud_state == CloudState::Placeholder {
        return fail("file is not downloaded".to_owned());
    }
    let existing = shared.cache.path_for(record.fingerprint, level);
    if existing.is_file() {
        let _ = shared.events.send(LibraryEvent::ThumbReady {
            id,
            level,
            path: existing,
        });
        return Ok(());
    }
    let image = match shared
        .backends
        .decode_scaled(&record.path, level.long_edge())
    {
        Ok(img) => img,
        Err(e) => return fail(e.to_string()),
    };
    ctx.check()?;
    let path = match shared.cache.write(record.fingerprint, level, &image) {
        Ok(p) => p,
        Err(e) => return fail(e.to_string()),
    };
    let thumb = ThumbRecord {
        media_id: id,
        level,
        path: path.clone(),
        width: image.width,
        height: image.height,
        generated_at_ms: now_ms(),
    };
    if let Err(e) = lock(&shared.catalogue).set_thumb(&thumb) {
        tracing::warn!(%id, error = %e, "could not record thumbnail");
    }
    let _ = shared
        .events
        .send(LibraryEvent::ThumbReady { id, level, path });
    Ok(())
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Current wall-clock time in Unix milliseconds.
#[must_use]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use crate::query::Query;
    use crate::record::ProbeState;
    use clipforge_media::MediaKind;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
    }

    fn library(dir: &std::path::Path) -> Library {
        let scheduler = Arc::new(Scheduler::new(0));
        Library::with_parts(
            Catalogue::open_in_memory().unwrap(),
            ThumbCache::new(dir.join("thumbs")),
            Backends::discover(),
            scheduler,
        )
    }

    fn run_all(lib: &Library) {
        while lib.scheduler().run_one() {}
    }

    fn drain(lib: &Library) -> Vec<LibraryEvent> {
        lib.events().try_iter().collect()
    }

    #[test]
    fn import_registers_probes_and_thumbnails_fixtures() {
        let dir = tempfile::tempdir().unwrap();
        let lib = library(dir.path());
        let has_ffmpeg = Backends::discover().has_ffmpeg();
        lib.import(vec![fixtures()]);
        run_all(&lib);
        let events = drain(&lib);

        assert_eq!(
            events.first(),
            Some(&LibraryEvent::ImportStarted { total: 11 })
        );
        assert!(events.contains(&LibraryEvent::ImportFinished {
            added: 11,
            existing: 0,
            failed: 0
        }));
        let added: usize = events
            .iter()
            .filter_map(|e| match e {
                LibraryEvent::ItemsAdded(v) => Some(v.len()),
                _ => None,
            })
            .sum();
        assert_eq!(added, 11);

        let cat = lib.catalogue();
        assert_eq!(cat.len().unwrap(), 11);
        let by_name = |n: &str| {
            cat.query(&Query::all().text(n))
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
        };

        let jpg = by_name("photo_landscape");
        assert_eq!(jpg.probe, ProbeState::Done);
        assert_eq!(jpg.info.as_ref().unwrap().width, Some(320));
        assert!(
            cat.thumb(jpg.id, ThumbLevel::Small).unwrap().is_some(),
            "eager small thumbnail"
        );

        let broken = by_name("broken_truncated");
        assert!(matches!(broken.probe, ProbeState::Failed(_)));
        assert!(cat.thumb(broken.id, ThumbLevel::Small).unwrap().is_none());

        let cloud = by_name("photo_cloud_only");
        assert_eq!(cloud.cloud_state, CloudState::Placeholder);
        assert_eq!(
            cloud.probe,
            ProbeState::Pending,
            "placeholders are never read"
        );
        assert_eq!(cloud.kind, MediaKind::Photo);

        let video = by_name("video_sdr_h264");
        if has_ffmpeg {
            assert_eq!(video.probe, ProbeState::Done);
            assert!(video.info.unwrap().has_audio);
            assert!(cat.thumb(video.id, ThumbLevel::Small).unwrap().is_some());
        } else {
            eprintln!("ffmpeg missing; video probe expected to fail");
            assert!(matches!(video.probe, ProbeState::Failed(_)));
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LibraryEvent::ItemUpdated(id) if *id == jpg.id))
        );
        assert!(events.iter().any(|e| matches!(e, LibraryEvent::ThumbReady { id, level: ThumbLevel::Small, .. } if *id == jpg.id)));
    }

    #[test]
    fn reimport_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let lib = library(dir.path());
        lib.import(vec![fixtures()]);
        run_all(&lib);
        drain(&lib);
        lib.import(vec![fixtures().join("photo_landscape.jpg"), fixtures()]);
        run_all(&lib);
        let events = drain(&lib);
        assert!(events.contains(&LibraryEvent::ImportFinished {
            added: 0,
            existing: 12,
            failed: 0
        }));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, LibraryEvent::ItemsAdded(_)))
        );
        assert_eq!(lib.catalogue().len().unwrap(), 11);
    }

    #[test]
    fn request_thumb_returns_cached_path_or_schedules() {
        let dir = tempfile::tempdir().unwrap();
        let lib = library(dir.path());
        lib.import(vec![fixtures().join("photo_landscape.jpg")]);
        run_all(&lib);
        drain(&lib);
        let id = lib.catalogue().query(&Query::all()).unwrap()[0].id;

        let small = lib.request_thumb(id, ThumbLevel::Small, Priority::Interactive);
        assert!(
            small.is_some_and(|p| p.is_file()),
            "eager small thumb is served synchronously"
        );

        assert!(
            lib.request_thumb(id, ThumbLevel::Medium, Priority::Idle)
                .is_none()
        );
        assert!(
            lib.request_thumb(id, ThumbLevel::Medium, Priority::Interactive)
                .is_none(),
            "deduplicated"
        );
        assert_eq!(lib.scheduler().queued(), 1);
        run_all(&lib);
        let events = drain(&lib);
        let ready = events.iter().find_map(|e| match e {
            LibraryEvent::ThumbReady {
                id: i,
                level: ThumbLevel::Medium,
                path,
            } if *i == id => Some(path.clone()),
            _ => None,
        });
        let path = ready.expect("medium thumbnail ready");
        assert!(path.is_file());
        assert_eq!(
            lib.request_thumb(id, ThumbLevel::Medium, Priority::Idle),
            Some(path)
        );
        let rec = lib
            .catalogue()
            .thumb(id, ThumbLevel::Medium)
            .unwrap()
            .unwrap();
        assert_eq!(
            (rec.width, rec.height),
            (320, 180),
            "never upscaled beyond source"
        );
    }

    #[test]
    fn placeholder_thumbs_fail_without_reading() {
        let dir = tempfile::tempdir().unwrap();
        let lib = library(dir.path());
        lib.import(vec![fixtures().join(".photo_cloud_only.jpg.icloud")]);
        run_all(&lib);
        drain(&lib);
        let id = lib.catalogue().query(&Query::all()).unwrap()[0].id;
        assert!(
            lib.request_thumb(id, ThumbLevel::Small, Priority::Interactive)
                .is_none()
        );
        run_all(&lib);
        assert!(
            drain(&lib)
                .iter()
                .any(|e| matches!(e, LibraryEvent::ThumbFailed { id: i, .. } if *i == id))
        );
    }

    #[test]
    fn remove_clears_catalogue_and_cache() {
        let dir = tempfile::tempdir().unwrap();
        let lib = library(dir.path());
        lib.import(vec![fixtures().join("photo_landscape.jpg")]);
        run_all(&lib);
        let rec = lib.catalogue().query(&Query::all()).unwrap().remove(0);
        assert!(lib.cache().exists(rec.fingerprint, ThumbLevel::Small));
        assert_eq!(lib.remove(&[rec.id]).unwrap(), 1);
        assert!(lib.catalogue().is_empty().unwrap());
        assert!(!lib.cache().exists(rec.fingerprint, ThumbLevel::Small));
    }

    #[test]
    fn cancelled_import_stops_early() {
        let dir = tempfile::tempdir().unwrap();
        let lib = library(dir.path());
        let handle = lib.import(vec![fixtures()]);
        handle.cancel();
        run_all(&lib);
        assert!(lib.catalogue().is_empty().unwrap());
        assert!(
            !drain(&lib)
                .iter()
                .any(|e| matches!(e, LibraryEvent::ImportFinished { .. }))
        );
    }
}
