//! Preview playback: background frame fetchers per video and an audio
//! output fed from the timeline. Everything public here is called on the
//! UI thread; the work happens on worker threads.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use clipforge_core::timeline::total_duration;
use clipforge_core::{MediaId, Project, Ticks};
use clipforge_export::audio::{AudioSourceFactory, AudioStream, Mixer};
use clipforge_media::{
    AUDIO_CHANNELS, AUDIO_SAMPLE_RATE, AudioReader, FfmpegCli, MediaInfo, VideoReader,
};
use clipforge_render::SourceImage;
use tracing::{debug, warn};

/// Fetchers idle longer than this are shut down.
const FETCHER_IDLE: Duration = Duration::from_secs(4);
/// Requests further ahead than this reopen the reader instead of decoding through.
const MAX_SKIP: Ticks = Ticks::from_seconds(3);
/// Audio ring buffer size in stereo frames (0.4 s).
const RING_FRAMES: usize = 19_200;

struct FetcherShared {
    wanted: Mutex<Option<Ticks>>,
    latest: Mutex<Option<(Ticks, SourceImage)>>,
    changed: AtomicBool,
    stop: AtomicBool,
    cv: Condvar,
}

/// One background thread streaming frames of one video.
struct FrameFetcher {
    shared: Arc<FetcherShared>,
    last_request: Instant,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FrameFetcher {
    fn start(
        cli: FfmpegCli,
        path: std::path::PathBuf,
        info: MediaInfo,
        max_edge: u32,
    ) -> FrameFetcher {
        let shared = Arc::new(FetcherShared {
            wanted: Mutex::new(None),
            latest: Mutex::new(None),
            changed: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            cv: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("clipforge-video-fetch".into())
            .spawn(move || fetch_loop(&worker, &cli, &path, &info, max_edge))
            .ok();
        FrameFetcher {
            shared,
            last_request: Instant::now(),
            thread,
        }
    }

    fn request(&mut self, t: Ticks) {
        self.last_request = Instant::now();
        if let Ok(mut w) = self.shared.wanted.lock() {
            *w = Some(t);
        }
        self.shared.cv.notify_one();
    }

    fn latest(&self) -> Option<(Ticks, SourceImage)> {
        self.shared.latest.lock().ok().and_then(|l| l.clone())
    }
}

impl Drop for FrameFetcher {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn fetch_loop(
    shared: &FetcherShared,
    cli: &FfmpegCli,
    path: &std::path::Path,
    info: &MediaInfo,
    max_edge: u32,
) {
    let mut reader: Option<VideoReader> = None;
    loop {
        let wanted = {
            let Ok(mut guard) = shared.wanted.lock() else {
                return;
            };
            loop {
                if shared.stop.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(t) = guard.take() {
                    break t;
                }
                let Ok(g) = shared.cv.wait(guard) else { return };
                guard = g;
            }
        };
        let reopen = reader
            .as_ref()
            .is_none_or(|r| !r.can_reach(wanted) || wanted > r.next_pts() + MAX_SKIP);
        if reopen {
            match VideoReader::open(cli, path, info, wanted, max_edge) {
                Ok(r) => reader = Some(r),
                Err(e) => {
                    warn!(error = %e, "cannot open video for preview");
                    reader = None;
                    continue;
                }
            }
        }
        if let Some(r) = reader.as_mut() {
            match r.frame_at(wanted) {
                Ok(Some(img)) => {
                    let src = SourceImage {
                        width: img.width,
                        height: img.height,
                        rgba: Arc::new(img.rgba),
                    };
                    if let Ok(mut l) = shared.latest.lock() {
                        *l = Some((wanted, src));
                    }
                    shared.changed.store(true, Ordering::SeqCst);
                }
                Ok(None) => {}
                Err(e) => {
                    debug!(error = %e, "video read failed; will reopen");
                    reader = None;
                }
            }
        }
    }
}

/// True if playing from `from` would make any sound.
pub(crate) fn audible_from(project: &Project, from: Ticks) -> bool {
    from < total_duration(&project.clips) && clipforge_export::audio::has_audio(project)
}

/// Opens preview audio streams with the ffmpeg sidecar.
struct ReaderFactory {
    cli: FfmpegCli,
    paths: HashMap<MediaId, std::path::PathBuf>,
}

impl AudioSourceFactory for ReaderFactory {
    fn open(&self, media: MediaId, start: Ticks) -> Option<Box<dyn AudioStream>> {
        let path = self.paths.get(&media)?;
        let reader = AudioReader::open(&self.cli, path, start).ok()?;
        Some(Box::new(ReaderStream(reader)))
    }
}

struct ReaderStream(AudioReader);

impl AudioStream for ReaderStream {
    fn read(&mut self, buf: &mut [f32]) -> usize {
        self.0.read(buf).unwrap_or(0)
    }
}

struct AudioOutput {
    _stream: cpal::Stream,
    ring: Arc<Mutex<VecDeque<f32>>>,
    feeder_stop: Arc<AtomicBool>,
    feeder: Option<std::thread::JoinHandle<()>>,
}

impl AudioOutput {
    fn open() -> Option<AudioOutput> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let device = cpal::default_host().default_output_device()?;
        let config = cpal::StreamConfig {
            channels: 2,
            sample_rate: AUDIO_SAMPLE_RATE,
            buffer_size: cpal::BufferSize::Default,
        };
        let ring: Arc<Mutex<VecDeque<f32>>> =
            Arc::new(Mutex::new(VecDeque::with_capacity(RING_FRAMES * 2)));
        let ring_cb = Arc::clone(&ring);
        let stream = device
            .build_output_stream(
                config,
                move |out: &mut [f32], _| {
                    let mut r = ring_cb
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for v in out.iter_mut() {
                        *v = r.pop_front().unwrap_or(0.0);
                    }
                },
                |e| warn!(error = %e, "audio stream error"),
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(AudioOutput {
            _stream: stream,
            ring,
            feeder_stop: Arc::new(AtomicBool::new(false)),
            feeder: None,
        })
    }

    fn stop_feeder(&mut self) {
        self.feeder_stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.feeder.take() {
            let _ = t.join();
        }
        if let Ok(mut r) = self.ring.lock() {
            r.clear();
        }
    }

    fn start_feeder(&mut self, factory: ReaderFactory, project: Project, from: Ticks) {
        self.stop_feeder();
        let stop = Arc::new(AtomicBool::new(false));
        self.feeder_stop = Arc::clone(&stop);
        let ring = Arc::clone(&self.ring);
        self.feeder = std::thread::Builder::new()
            .name("clipforge-audio-feed".into())
            .spawn(move || feed_loop(&stop, &ring, &factory, &project, from))
            .ok();
    }
}

/// Mixes the timeline from `from` into the ring, staying at most
/// `RING_FRAMES` ahead of the output.
fn feed_loop(
    stop: &AtomicBool,
    ring: &Mutex<VecDeque<f32>>,
    factory: &ReaderFactory,
    project: &Project,
    from: Ticks,
) {
    let mut mixer = Mixer::new(project, factory, from);
    let mut buf = vec![0.0f32; 2048 * AUDIO_CHANNELS];
    loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let room = {
            let r = ring
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            RING_FRAMES.saturating_sub(r.len() / AUDIO_CHANNELS)
        };
        if room < 1024 {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        let want = room.min(2048) * AUDIO_CHANNELS;
        let n = mixer.fill(&mut buf[..want]);
        if n == 0 {
            return;
        }
        ring.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(&buf[..n]);
    }
}

/// Owns fetchers and audio for the preview.
pub(crate) struct Player {
    cli: Option<FfmpegCli>,
    fetchers: Mutex<HashMap<MediaId, FrameFetcher>>,
    audio: Mutex<Option<AudioOutput>>,
    audio_failed: AtomicBool,
    max_edge: u32,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player").finish()
    }
}

impl Player {
    pub(crate) fn new(cli: Option<FfmpegCli>, max_edge: u32) -> Player {
        Player {
            cli,
            fetchers: Mutex::new(HashMap::new()),
            audio: Mutex::new(None),
            audio_failed: AtomicBool::new(false),
            max_edge,
        }
    }

    /// Latest frame for `media`, requesting `t` for the next update.
    /// `open` supplies path and info when a fetcher has to be created.
    pub(crate) fn video_frame(
        &self,
        media: MediaId,
        t: Ticks,
        open: impl FnOnce() -> Option<(std::path::PathBuf, MediaInfo)>,
    ) -> Option<SourceImage> {
        let cli = self.cli.as_ref()?;
        let mut fetchers = self.fetchers.lock().ok()?;
        let f = match fetchers.entry(media) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let (path, info) = open()?;
                e.insert(FrameFetcher::start(cli.clone(), path, info, self.max_edge))
            }
        };
        let latest = f.latest();
        if latest.as_ref().is_none_or(|(have, _)| *have != t) {
            f.request(t);
        }
        latest.map(|(_, img)| img)
    }

    /// True if a fetcher produced a new frame since the last call.
    pub(crate) fn take_changed(&self) -> bool {
        let Ok(fetchers) = self.fetchers.lock() else {
            return false;
        };
        // Every flag must be cleared, so no short-circuiting `any`.
        let mut changed = false;
        for f in fetchers.values() {
            changed |= f.shared.changed.swap(false, Ordering::SeqCst);
        }
        changed
    }

    /// Drops fetchers nobody asked for recently.
    pub(crate) fn prune(&self) {
        if let Ok(mut fetchers) = self.fetchers.lock() {
            fetchers.retain(|_, f| f.last_request.elapsed() < FETCHER_IDLE);
        }
    }

    /// Starts audio for the timeline from `from`.
    pub(crate) fn play_audio(&self, project: &Project, from: Ticks) {
        let Some(cli) = self.cli.clone() else { return };
        if !audible_from(project, from) {
            self.stop_audio();
            return;
        }
        let Ok(mut audio) = self.audio.lock() else {
            return;
        };
        if audio.is_none() && !self.audio_failed.load(Ordering::SeqCst) {
            *audio = AudioOutput::open();
            if audio.is_none() {
                warn!("no audio output device; playing silently");
                self.audio_failed.store(true, Ordering::SeqCst);
            }
        }
        if let Some(out) = audio.as_mut() {
            let paths = project
                .media
                .values()
                .map(|m| (m.id, m.path.clone()))
                .collect();
            out.start_feeder(ReaderFactory { cli, paths }, project.clone(), from);
        }
    }

    pub(crate) fn stop_audio(&self) {
        if let Ok(mut audio) = self.audio.lock()
            && let Some(out) = audio.as_mut()
        {
            out.stop_feeder();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::project::{ClipSource, MediaRef, RefKind, Transition, TransitionKind};
    use clipforge_core::{Clip, Command};

    fn project() -> Project {
        let mut p = Project::new();
        let mut media = Vec::new();
        let mut entries = Vec::new();
        for i in 0..3 {
            let id = MediaId::new();
            media.push(MediaRef {
                id,
                kind: RefKind::Video,
                path: format!("/v{i}").into(),
                fingerprint_hash: 1,
                size: 1,
                pixel_size: Some((16, 9)),
                duration: Some(Ticks::from_seconds(10)),
                captured_at_ms: None,
                name: "v".into(),
            });
            let mut c = Clip::video(id, Ticks::from_seconds(10));
            c.source = ClipSource::Video {
                in_point: Ticks::from_seconds(1),
                out_point: Ticks::from_seconds(3),
            };
            if i == 2 {
                c.transition_in = Transition {
                    kind: TransitionKind::CrossDissolve,
                    duration: Ticks::SECOND,
                };
            }
            entries.push((i, c));
        }
        Command::InsertClips { entries, media }
            .apply(&mut p)
            .unwrap();
        p
    }

    #[test]
    fn audio_plays_only_where_something_sounds() {
        let mut p = project();
        assert!(audible_from(&p, Ticks::ZERO));
        assert!(!audible_from(&p, Ticks::from_seconds(10)), "past the end");
        Command::SetMuted {
            indices: vec![0, 1, 2],
            muted: true,
        }
        .apply(&mut p)
        .unwrap();
        assert!(!audible_from(&p, Ticks::ZERO));
    }
}
