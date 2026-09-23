//! Full-resolution source pixels for export, decoded from the original
//! files with the media backends. A small cache keeps the most recent
//! pictures because transitions read two clips alternately.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clipforge_core::{MediaId, Project};
use clipforge_media::{Backends, StillDecoder};
use clipforge_render::{SourceImage, SourceProvider};

const CACHE_ENTRIES: usize = 8;

#[derive(Debug)]
pub struct FileSources {
    backends: Backends,
    paths: HashMap<MediaId, PathBuf>,
    cache: Mutex<HashMap<MediaId, SourceImage>>,
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
            paths,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

impl SourceProvider for FileSources {
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
            .run(&plan, &mut frames, &out, &CancellationToken::new(), |_| {})
            .unwrap();
        // 1.6 s total at 25 fps = 40 frames.
        assert_eq!(report.frames, 40);
        let info = clipforge_media::FfmpegCli::new(loc).probe(&out).unwrap();
        assert_eq!((info.width, info.height), (Some(960), Some(540)));
        let d = info.duration.unwrap().as_seconds_f64();
        assert!((1.5..=1.7).contains(&d), "{d}");
    }
}
