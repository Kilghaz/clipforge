//! Chooses a backend per file.

use std::path::Path;

use crate::error::{MediaError, Result};
use crate::ffmpeg_cli::FfmpegCli;
use crate::image_backend::ImageBackend;
use crate::info::MediaInfo;
use crate::kind::MediaKind;
use crate::probe::{DecodedImage, Prober, StillDecoder};

/// The default probe/decode stack: `image` crate for what it reads,
/// ffmpeg for the rest. Works without ffmpeg for plain photos.
#[derive(Debug, Clone)]
pub struct Backends {
    image: ImageBackend,
    ffmpeg: Option<FfmpegCli>,
}

impl Backends {
    #[must_use]
    pub fn new(ffmpeg: Option<FfmpegCli>) -> Self {
        Backends {
            image: ImageBackend,
            ffmpeg,
        }
    }

    /// Discovers ffmpeg automatically.
    #[must_use]
    pub fn discover() -> Self {
        Self::new(FfmpegCli::discover())
    }

    #[must_use]
    pub fn has_ffmpeg(&self) -> bool {
        self.ffmpeg.is_some()
    }

    fn ffmpeg_or_unsupported(&self, path: &Path) -> Result<&FfmpegCli> {
        self.ffmpeg.as_ref().ok_or_else(|| MediaError::ToolMissing {
            tool: "ffmpeg".into(),
            detail: format!("needed for {}", path.display()),
        })
    }

    /// True if some backend can handle this file type at all.
    #[must_use]
    pub fn can_handle(&self, path: &Path) -> bool {
        ImageBackend::supports(path)
            || (self.ffmpeg.is_some() && MediaKind::from_path(path) != MediaKind::Unknown)
    }
}

impl Prober for Backends {
    fn probe(&self, path: &Path) -> Result<MediaInfo> {
        // The image crate's verdict is final for its formats: ffprobe accepts
        // files it never decodes, which would turn garbage into "photos".
        if ImageBackend::supports(path) {
            return self.image.probe(path);
        }
        self.ffmpeg_or_unsupported(path)?.probe(path)
    }
}

impl StillDecoder for Backends {
    fn decode_scaled(&self, path: &Path, max_edge: u32) -> Result<DecodedImage> {
        if ImageBackend::supports(path) {
            match self.image.decode_scaled(path, max_edge) {
                Ok(img) => return Ok(img),
                // ffmpeg reads some variants the image crate rejects (e.g.
                // unusual TIFF layouts); it fails properly on real garbage.
                Err(e) if self.ffmpeg.is_none() => return Err(e),
                Err(_) => {}
            }
        }
        self.ffmpeg_or_unsupported(path)?
            .decode_scaled(path, max_edge)
    }
}

#[cfg(test)]
#[allow(clippy::print_stderr)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn without_ffmpeg_photos_work_and_videos_report_missing_tool() {
        let b = Backends::new(None);
        assert!(b.probe(&fixture("photo_landscape.jpg")).is_ok());
        assert!(matches!(
            b.probe(&fixture("video_sdr_h264.mp4")),
            Err(MediaError::ToolMissing { .. })
        ));
        assert!(b.can_handle(Path::new("a.png")));
        assert!(!b.can_handle(Path::new("a.mp4")));
        assert!(!b.can_handle(Path::new("a.txt")));
    }

    #[test]
    fn with_ffmpeg_everything_routes() {
        let b = Backends::discover();
        if !b.has_ffmpeg() {
            eprintln!("ffmpeg not installed; skipping");
            return;
        }
        assert!(b.can_handle(Path::new("a.heic")));
        assert!(b.can_handle(Path::new("a.mp4")));
        assert_eq!(
            b.probe(&fixture("video_sdr_h264.mp4")).unwrap().kind,
            MediaKind::Video
        );
        assert_eq!(
            b.probe(&fixture("photo_landscape.jpg")).unwrap().codec,
            "jpeg"
        );
        assert!(b.probe(&fixture("broken_truncated.jpg")).is_err());
        assert!(
            b.decode_scaled(&fixture("broken_truncated.jpg"), 64)
                .is_err()
        );
    }
}
