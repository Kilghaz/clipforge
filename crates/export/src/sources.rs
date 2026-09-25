//! Full-resolution source pixels for export, decoded from the original
//! files with the media backends. A small cache keeps the most recent
//! pictures because transitions read two clips alternately.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clipforge_core::{MediaId, Project, Ticks};
use clipforge_media::{
    AudioReader, Backends, FfmpegCli, MediaInfo, Prober, StillDecoder, VideoFrame, VideoReader,
};
use clipforge_render::{HlgImage, SourceImage, SourceProvider, colour};

use crate::audio::{AudioSourceFactory, AudioStream};

const CACHE_ENTRIES: usize = 8;

/// A decoded video frame as an SDR picture. HDR frames are tone mapped in
/// horizontal bands on all cores (a 4K frame takes ~270 ms on one).
#[must_use]
pub fn sdr_frame(frame: VideoFrame) -> SourceImage {
    clipforge_render::sdr_source_with(frame, |rgb48, transfer, out| {
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        // Whole pixels per band: 3 samples in, 4 bytes out.
        let pixels = (rgb48.len() / 3).div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            for (src, dst) in rgb48.chunks(pixels * 3).zip(out.chunks_mut(pixels * 4)) {
                scope.spawn(move || colour::hdr16_to_sdr8_into(src, transfer, dst));
            }
        });
    })
}

pub struct FileSources {
    backends: Backends,
    ffmpeg: Option<FfmpegCli>,
    paths: HashMap<MediaId, PathBuf>,
    cache: Mutex<HashMap<MediaId, SourceImage>>,
    infos: Mutex<HashMap<MediaId, Option<MediaInfo>>>,
    /// One streaming reader per video, reopened when a request goes backwards.
    readers: Mutex<HashMap<MediaId, VideoReader>>,
}

impl std::fmt::Debug for FileSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSources")
            .field("media", &self.paths.len())
            .finish()
    }
}

impl FileSources {
    /// Uses the paths recorded in the project's media table.
    #[must_use]
    pub fn for_project(project: &Project, backends: Backends) -> FileSources {
        let paths = project
            .media
            .values()
            .map(|m| (m.id, m.path.clone()))
            .collect();
        FileSources {
            backends,
            ffmpeg: FfmpegCli::discover(),
            paths,
            cache: Mutex::new(HashMap::new()),
            infos: Mutex::new(HashMap::new()),
            readers: Mutex::new(HashMap::new()),
        }
    }

    fn info(&self, media: MediaId) -> Option<MediaInfo> {
        if let Some(cached) = self.infos.lock().ok().and_then(|m| m.get(&media).cloned()) {
            return cached;
        }
        let info = self
            .paths
            .get(&media)
            .and_then(|p| self.backends.probe(p).ok());
        if let Ok(mut m) = self.infos.lock() {
            m.insert(media, info.clone());
        }
        info
    }
}

impl AudioSourceFactory for FileSources {
    fn open(&self, media: MediaId, start: Ticks) -> Option<Box<dyn AudioStream>> {
        let cli = self.ffmpeg.as_ref()?;
        let path = self.paths.get(&media)?;
        if !self.info(media)?.has_audio {
            return None;
        }
        let reader = AudioReader::open(cli, path, start).ok()?;
        Some(Box::new(ReaderStream(reader)))
    }
}

struct ReaderStream(AudioReader);

impl AudioStream for ReaderStream {
    fn read(&mut self, buf: &mut [f32]) -> usize {
        self.0.read(buf).unwrap_or(0)
    }
}

impl FileSources {
    /// The decoded frame of `media` at `source_time`, reusing the streaming
    /// reader while the requests move forward.
    fn frame(&self, media: MediaId, source_time: Ticks, max_edge: u32) -> Option<VideoFrame> {
        let cli = self.ffmpeg.as_ref()?;
        let path = self.paths.get(&media)?;
        let info = self.info(media)?;
        let (dw, dh) = info.display_size()?;
        let wanted_edge = max_edge.min(dw.max(dh));
        let mut readers = self.readers.lock().ok()?;
        let needs_new = readers.get(&media).is_none_or(|r| {
            let (rw, rh) = r.size();
            !r.can_reach(source_time) || rw.max(rh) < wanted_edge
        });
        if needs_new {
            let reader = VideoReader::open(cli, path, &info, source_time, max_edge).ok()?;
            readers.insert(media, reader);
        }
        let reader = readers.get_mut(&media)?;
        reader.frame_at(source_time).ok()?
    }
}

impl SourceProvider for FileSources {
    /// SDR output: HDR videos tone mapped.
    fn video_frame(
        &self,
        media: MediaId,
        source_time: Ticks,
        max_edge: u32,
    ) -> Option<SourceImage> {
        self.frame(media, source_time, max_edge).map(sdr_frame)
    }

    /// HDR output: HDR videos at full range (HLG); SDR videos come through
    /// `video_frame` and are converted by the compositor.
    fn video_frame_hlg(
        &self,
        media: MediaId,
        source_time: Ticks,
        max_edge: u32,
    ) -> Option<HlgImage> {
        let frame = self.frame(media, source_time, max_edge)?;
        clipforge_render::hlg_source(&frame)
    }

    fn still(&self, media: MediaId, max_edge: u32) -> Option<SourceImage> {
        if let Some(img) = self.cache.lock().ok().and_then(|c| c.get(&media).cloned()) {
            return Some(img);
        }
        let path = self.paths.get(&media)?;
        let decoded = match self.backends.decode_scaled(path, max_edge) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(%media, error = %e, "cannot decode source for export");
                return None;
            }
        };
        let img = SourceImage {
            width: decoded.width,
            height: decoded.height,
            rgba: Arc::new(decoded.rgba),
        };
        if let Ok(mut c) = self.cache.lock() {
            if c.len() >= CACHE_ENTRIES {
                c.clear();
            }
            c.insert(media, img.clone());
        }
        Some(img)
    }
}

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use crate::{EncodePlan, ExportOptions, Exporter, TimelineFrames};
    use clipforge_core::project::{MediaRef, RefKind, Transition, TransitionKind};
    use clipforge_core::{Aspect, Clip, Command, FrameRate, Ticks};
    use clipforge_jobs::CancellationToken;
    use clipforge_media::{FfmpegLocation, Prober};
    use clipforge_render::{Compositor, RenderQuality};

    fn hlg_frame(width: u32, height: u32) -> VideoFrame {
        let n = (width * height) as usize;
        let rgb48 = (0..n * 3).map(|i| (i * 131 % 65_536) as u16).collect();
        VideoFrame::Hdr(clipforge_media::HdrImage {
            width,
            height,
            transfer: clipforge_media::ColorTransfer::Hlg,
            rgb48,
        })
    }

    #[test]
    fn parallel_tone_mapping_matches_the_single_threaded_one() {
        // An odd size so the bands do not divide evenly.
        let frame = hlg_frame(37, 23);
        let parallel = sdr_frame(frame.clone());
        let single = clipforge_render::sdr_source(frame);
        assert_eq!(parallel.rgba, single.rgba);
        assert_eq!(parallel.rgba.len(), 37 * 23 * 4);
    }

    #[test]
    #[ignore = "timing, run manually"]
    #[allow(clippy::print_stderr)]
    fn parallel_tone_mapping_frame_time() {
        let frame = hlg_frame(3840, 2160);
        let start = std::time::Instant::now();
        for _ in 0..5 {
            std::hint::black_box(sdr_frame(frame.clone()));
        }
        eprintln!(
            "parallel tone map 4K: {:.1} ms/frame",
            start.elapsed().as_secs_f64() * 200.0
        );
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name)
    }

    fn media_ref(path: PathBuf) -> MediaRef {
        MediaRef {
            id: MediaId::new(),
            kind: RefKind::Photo,
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
            fingerprint_hash: 1,
            size: 1,
            pixel_size: None,
            duration: None,
            captured_at_ms: None,
        }
    }

    #[test]
    fn decodes_real_files_and_caches() {
        let a = media_ref(fixture("photo_landscape.jpg"));
        let mut p = Project::new();
        Command::InsertClips {
            entries: vec![(0, Clip::photo(a.id, Ticks::SECOND))],
            media: vec![a.clone()],
        }
        .apply(&mut p)
        .unwrap();
        let s = FileSources::for_project(&p, Backends::new(None));
        let img = s.still(a.id, 4000).unwrap();
        assert_eq!((img.width, img.height), (320, 180));
        assert!(s.still(MediaId::new(), 100).is_none());
        assert_eq!(s.cache.lock().unwrap().len(), 1);
    }

    /// Music path: a real song under photos, looped past its end.
    #[test]
    fn mixes_a_real_song_under_photos() {
        if FfmpegLocation::discover().is_none() {
            eprintln!("ffmpeg not installed; skipping");
            return;
        }
        let photo = media_ref(fixture("photo_landscape.jpg"));
        let mut song = media_ref(fixture("audio_stereo.m4a"));
        song.kind = RefKind::Audio;
        song.duration = Some(Ticks::from_seconds(3));
        let mut p = Project::new();
        Command::InsertClips {
            entries: vec![(0, Clip::photo(photo.id, Ticks::from_seconds(4)))],
            media: vec![photo],
        }
        .apply(&mut p)
        .unwrap();
        let music = clipforge_core::Music {
            songs: vec![clipforge_core::Song::new(song.id)],
            fade_out: Ticks::ZERO,
            ..clipforge_core::Music::default()
        };
        Command::SetMusic {
            music,
            media: vec![song],
        }
        .apply(&mut p)
        .unwrap();
        let sources = FileSources::for_project(&p, Backends::discover());
        let samples = crate::audio::mix(&p, &sources);
        assert_eq!(samples.len(), 4 * 48_000 * 2);
        let rms = |range: std::ops::Range<usize>| {
            let s = &samples[range.start * 96_000..range.end * 96_000];
            (s.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / s.len() as f64).sqrt()
        };
        assert!(rms(1..2) > 0.05, "song plays: {}", rms(1..2));
        assert!(rms(3..4) > 0.05, "and loops: {}", rms(3..4));
    }

    /// Video path: a trimmed clip with its audio, through readers, mix and ffmpeg.
    #[test]
    fn exports_a_trimmed_video_clip_with_audio() {
        let Some(loc) = FfmpegLocation::discover() else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        let mut v = media_ref(fixture("video_sdr_h264.mp4"));
        v.kind = RefKind::Video;
        v.duration = Some(Ticks::from_seconds(2));
        let mut p = Project::new();
        p.settings.frame_rate = FrameRate::FPS_25;
        let mut clip = Clip::video(v.id, Ticks::from_seconds(2));
        clip.source = clipforge_core::ClipSource::Video {
            in_point: Ticks::from_millis(500),
            out_point: Ticks::from_millis(1_500),
        };
        Command::InsertClips {
            entries: vec![(0, clip)],
            media: vec![v],
        }
        .apply(&mut p)
        .unwrap();

        let sources = FileSources::for_project(&p, Backends::discover());
        assert!(crate::audio::has_audio(&p));
        let samples = crate::audio::mix(&p, &sources);
        assert_eq!(samples.len(), 48_000 * 2, "one second of stereo");
        let rms = (samples
            .iter()
            .map(|s| f64::from(*s) * f64::from(*s))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        assert!(rms > 0.05, "tone present: {rms}");

        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("mix.wav");
        crate::audio::write_wav(&wav, &samples).unwrap();
        let out = dir.path().join("video.mp4");
        let mut plan = EncodePlan::build(
            &ExportOptions::default(),
            Aspect::Landscape16x9,
            FrameRate::FPS_25,
        );
        plan.width = 960;
        plan.height = 540;
        plan.video_bitrate_kbps = 1500;
        let mut exporter = Exporter::new(loc.ffmpeg.clone()).unwrap();
        exporter.prefer_hardware = false;
        let compositor = Compositor::new();
        let mut frames = TimelineFrames::new(&p, &compositor, &sources, RenderQuality::Preview);
        let report = exporter
            .run(
                &plan,
                &mut frames,
                Some(&wav),
                &out,
                &CancellationToken::new(),
                |_| {},
            )
            .unwrap();
        assert_eq!(report.frames, 25);
        let info = clipforge_media::FfmpegCli::new(loc).probe(&out).unwrap();
        assert!(info.has_audio, "audio track muxed");
        assert_eq!((info.width, info.height), (Some(960), Some(540)));
        let d = info.duration.unwrap().as_seconds_f64();
        assert!((0.9..=1.1).contains(&d), "{d}");
        // The picture at the middle must not be the placeholder colour.
        let mid = compositor.render(
            &p,
            Ticks::from_millis(500),
            RenderQuality::Preview,
            &sources,
        );
        let px = mid.pixel(480, 270);
        assert_ne!(&px[..3], &[40, 42, 48], "video frame decoded");
    }

    /// The whole photo path: real JPEG/PNG files → compositor → yuv → ffmpeg → mp4.
    #[test]
    fn exports_a_two_photo_slideshow_with_a_dissolve() {
        let Some(loc) = FfmpegLocation::discover() else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        let a = media_ref(fixture("photo_landscape.jpg"));
        let b = media_ref(fixture("photo_portrait.png"));
        let mut p = Project::new();
        p.settings.frame_rate = FrameRate::FPS_25;
        let mut second = Clip::photo(b.id, Ticks::SECOND);
        second.transition_in = Transition {
            kind: TransitionKind::CrossDissolve,
            duration: Ticks::from_millis(400),
        };
        Command::InsertClips {
            entries: vec![(0, Clip::photo(a.id, Ticks::SECOND)), (1, second)],
            media: vec![a, b],
        }
        .apply(&mut p)
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("slideshow.mp4");
        let mut plan = EncodePlan::build(
            &ExportOptions::default(),
            Aspect::Landscape16x9,
            FrameRate::FPS_25,
        );
        // Preview quality renders 960x540; the plan must match the frames.
        plan.width = 960;
        plan.height = 540;
        plan.video_bitrate_kbps = 1500;
        let mut exporter = Exporter::new(loc.ffmpeg.clone()).unwrap();
        exporter.prefer_hardware = false;
        let compositor = Compositor::new();
        let sources = FileSources::for_project(&p, Backends::new(None));
        let mut frames = TimelineFrames::new(&p, &compositor, &sources, RenderQuality::Preview);
        let report = exporter
            .run(
                &plan,
                &mut frames,
                None,
                &out,
                &CancellationToken::new(),
                |_| {},
            )
            .unwrap();
        // 1.6 s total at 25 fps = 40 frames.
        assert_eq!(report.frames, 40);
        let info = clipforge_media::FfmpegCli::new(loc).probe(&out).unwrap();
        assert_eq!((info.width, info.height), (Some(960), Some(540)));
        let d = info.duration.unwrap().as_seconds_f64();
        assert!((1.5..=1.7).contains(&d), "{d}");
    }
}
