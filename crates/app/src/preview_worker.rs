//! Renders preview frames on a worker thread so the UI thread only uploads
//! the finished picture. Requests coalesce: only the newest playhead is
//! rendered when the worker falls behind.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use clipforge_core::{Project, Ticks};
use clipforge_render::{Frame, RenderQuality, SourceProvider};

struct Shared {
    request: Mutex<Option<(Arc<Project>, Ticks)>>,
    result: Mutex<Option<Frame>>,
    stop: AtomicBool,
    cv: Condvar,
}

pub(crate) struct PreviewWorker {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PreviewWorker {
    /// Starts the worker. `sources` must be usable from the worker thread.
    pub(crate) fn start(sources: Arc<dyn SourceProvider + Send + Sync>) -> PreviewWorker {
        let shared = Arc::new(Shared {
            request: Mutex::new(None),
            result: Mutex::new(None),
            stop: AtomicBool::new(false),
            cv: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("clipforge-preview".into())
            .spawn(move || {
                // GPU when available (created on this thread, once), else CPU.
                let compositor = clipforge_render::best_renderer();
                tracing::info!(renderer = compositor.name(), "preview renderer");
                loop {
                    let (project, t) = {
                        let Ok(mut guard) = worker.request.lock() else {
                            return;
                        };
                        loop {
                            if worker.stop.load(Ordering::SeqCst) {
                                return;
                            }
                            if let Some(r) = guard.take() {
                                break r;
                            }
                            let Ok(g) = worker.cv.wait(guard) else { return };
                            guard = g;
                        }
                    };
                    let frame =
                        compositor.render(&project, t, RenderQuality::Preview, sources.as_ref());
                    if let Ok(mut r) = worker.result.lock() {
                        *r = Some(frame);
                    }
                }
            })
            .ok();
        PreviewWorker { shared, thread }
    }

    /// Asks for the frame at `t`; replaces any pending request.
    pub(crate) fn request(&self, project: Arc<Project>, t: Ticks) {
        if let Ok(mut r) = self.shared.request.lock() {
            *r = Some((project, t));
        }
        self.shared.cv.notify_one();
    }

    /// The most recently rendered frame, if a new one is ready.
    pub(crate) fn take_frame(&self) -> Option<Frame> {
        self.shared.result.lock().ok().and_then(|mut r| r.take())
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_render::source::MapProvider;
    use std::time::{Duration, Instant};

    #[test]
    fn renders_requested_frames_and_coalesces() {
        let worker = PreviewWorker::start(Arc::new(MapProvider::default()));
        let project = Arc::new(Project::new());
        for i in 0..50 {
            worker.request(Arc::clone(&project), Ticks::from_millis(i));
        }
        // Generous: creating a GPU device while other GPU tests run in
        // parallel can take seconds; the deadline only guards against a hang.
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut frame = None;
        while frame.is_none() && Instant::now() < deadline {
            frame = worker.take_frame();
            std::thread::sleep(Duration::from_millis(5));
        }
        let frame = frame.expect("a frame arrives");
        assert_eq!((frame.width, frame.height), (960, 540));
        assert!(worker.take_frame().is_none(), "frames are taken once");
    }
}
