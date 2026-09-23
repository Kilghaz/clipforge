//! Pure-Rust backend for JPEG, PNG, GIF, WebP, TIFF and BMP via the `image`
//! crate.

use std::path::Path;

use fast_image_resize as fr;
use image::{DynamicImage, ImageReader};

use crate::error::{MediaError, Result};
use crate::exif;
use crate::info::{MediaInfo, Rotation};
use crate::kind::{ColorTransfer, MediaKind};
use crate::probe::{DecodedImage, Prober, StillDecoder, fit_within};

#[derive(Debug, Default, Clone, Copy)]
pub struct ImageBackend;

impl ImageBackend {
    /// Extensions this backend reads.
    pub const EXTENSIONS: &'static [&'static str] = &[
        "jpg", "jpeg", "jpe", "png", "gif", "webp", "tif", "tiff", "bmp",
    ];

    #[must_use]
    pub fn supports(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| Self::EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    }

    fn open(path: &Path) -> Result<ImageReader<std::io::BufReader<std::fs::File>>> {
        let reader = ImageReader::open(path).map_err(|e| MediaError::io(path, e))?;
        reader
            .with_guessed_format()
            .map_err(|e| MediaError::io(path, e))
    }
}

impl Prober for ImageBackend {
    fn probe(&self, path: &Path) -> Result<MediaInfo> {
        let reader = Self::open(path)?;
        let format = reader
            .format()
            .ok_or_else(|| MediaError::Unsupported(path.display().to_string()))?;
        let (width, height) = reader
            .into_dimensions()
            .map_err(|e| MediaError::corrupt(format_name(format), e))?;
        let summary = exif::read(path);
        Ok(MediaInfo {
            kind: MediaKind::Photo,
            width: Some(width),
            height: Some(height),
            rotation: summary.rotation,
            duration: None,
            frame_rate: None,
            has_audio: false,
            transfer: ColorTransfer::Sdr,
            captured_at_ms: summary.captured_at_ms,
            codec: format_name(format).to_owned(),
        })
    }
}

impl StillDecoder for ImageBackend {
    fn decode_scaled(&self, path: &Path, max_edge: u32) -> Result<DecodedImage> {
        let reader = Self::open(path)?;
        let format = reader
            .format()
            .ok_or_else(|| MediaError::Unsupported(path.display().to_string()))?;
        let img = reader
            .decode()
            .map_err(|e| MediaError::corrupt(format_name(format), e))?;
        let rotation = exif::read(path).rotation;
        scale_and_rotate(img, max_edge, rotation)
    }
}

/// Downscales (never upscales) and applies a rotation. Shared with the
/// ffmpeg backend, which hands over already decoded frames.
pub(crate) fn scale_and_rotate(
    img: DynamicImage,
    max_edge: u32,
    rotation: Rotation,
) -> Result<DecodedImage> {
    let (w, h) = (img.width(), img.height());
    let (tw, th) = fit_within(w, h, max_edge);
    let src = img.into_rgba8();
    let scaled: image::RgbaImage = if (tw, th) == (w, h) {
        src
    } else {
        let src_dyn = DynamicImage::ImageRgba8(src);
        let mut dst = DynamicImage::new_rgba8(tw, th);
        let mut resizer = fr::Resizer::new();
        resizer
            .resize(
                &src_dyn,
                &mut dst,
                &fr::ResizeOptions::new()
                    .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3)),
            )
            .map_err(|e| MediaError::corrupt("resize", e))?;
        dst.into_rgba8()
    };
    let rotated = match rotation {
        Rotation::None => scaled,
        Rotation::Cw90 => image::imageops::rotate90(&scaled),
        Rotation::Cw180 => image::imageops::rotate180(&scaled),
        Rotation::Cw270 => image::imageops::rotate270(&scaled),
    };
    let (width, height) = rotated.dimensions();
    Ok(DecodedImage {
        width,
        height,
        rgba: rotated.into_raw(),
    })
}

fn format_name(format: image::ImageFormat) -> &'static str {
    match format {
        image::ImageFormat::Jpeg => "jpeg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Tiff => "tiff",
        image::ImageFormat::Bmp => "bmp",
        _ => "image",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn probes_fixture_dimensions_and_codec() {
        let b = ImageBackend;
        let jpg = b.probe(&fixture("photo_landscape.jpg")).unwrap();
        assert_eq!((jpg.width, jpg.height), (Some(320), Some(180)));
        assert_eq!(jpg.codec, "jpeg");
        assert_eq!(jpg.kind, MediaKind::Photo);
        assert_eq!(jpg.duration, None);
        let png = b.probe(&fixture("photo_portrait.png")).unwrap();
        assert_eq!(png.display_size(), Some((90, 160)));
        let tif = b.probe(&fixture("photo_square.tif")).unwrap();
        assert_eq!(
            (tif.width, tif.height, tif.codec.as_str()),
            (Some(64), Some(64), "tiff")
        );
    }

    #[test]
    fn broken_and_empty_files_fail_cleanly() {
        let b = ImageBackend;
        assert!(matches!(
            b.probe(&fixture("broken_truncated.jpg")),
            Err(MediaError::Corrupt { .. })
        ));
        assert!(matches!(
            b.decode_scaled(&fixture("broken_truncated.jpg"), 100),
            Err(MediaError::Corrupt { .. })
        ));
        assert!(matches!(
            b.probe(Path::new("/nope/missing.jpg")),
            Err(MediaError::Io { .. })
        ));
    }

    #[test]
    fn decode_scaled_fits_and_never_upscales() {
        let b = ImageBackend;
        let small = b
            .decode_scaled(&fixture("photo_landscape.jpg"), 64)
            .unwrap();
        assert_eq!((small.width, small.height), (64, 36));
        assert_eq!(small.rgba.len(), 64 * 36 * 4);
        let same = b
            .decode_scaled(&fixture("photo_landscape.jpg"), 4096)
            .unwrap();
        assert_eq!((same.width, same.height), (320, 180));
    }

    #[test]
    fn rotation_is_applied_after_scaling() {
        let img = DynamicImage::new_rgba8(400, 200);
        let out = scale_and_rotate(img, 100, Rotation::Cw90).unwrap();
        assert_eq!((out.width, out.height), (50, 100));
        let out =
            scale_and_rotate(DynamicImage::new_rgba8(400, 200), 100, Rotation::Cw180).unwrap();
        assert_eq!((out.width, out.height), (100, 50));
    }

    #[test]
    fn supports_by_extension() {
        assert!(ImageBackend::supports(Path::new("a.JPG")));
        assert!(ImageBackend::supports(Path::new("a.webp")));
        assert!(!ImageBackend::supports(Path::new("a.heic")));
        assert!(!ImageBackend::supports(Path::new("a.mp4")));
    }
}
