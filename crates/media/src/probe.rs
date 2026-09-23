//! Traits every backend implements.

use std::path::Path;

use crate::error::Result;
use crate::info::MediaInfo;

/// Extracts metadata without decoding pixels where possible.
pub trait Prober: Send + Sync {
    fn probe(&self, path: &Path) -> Result<MediaInfo>;
}

/// An 8-bit sRGB image in memory, already rotated for display.
#[derive(Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for DecodedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl DecodedImage {
    /// Encodes as JPEG with the given quality (1–100).
    pub fn to_jpeg(&self, quality: u8) -> Result<Vec<u8>> {
        use image::codecs::jpeg::JpegEncoder;
        let mut out = Vec::new();
        let rgb: Vec<u8> = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        JpegEncoder::new_with_quality(&mut out, quality)
            .encode(
                &rgb,
                self.width,
                self.height,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| crate::MediaError::corrupt("jpeg", e))?;
        Ok(out)
    }

    /// Average colour, handy for placeholders while a thumbnail loads.
    #[must_use]
    pub fn average_rgb(&self) -> [u8; 3] {
        let n = u64::from(self.width) * u64::from(self.height);
        if n == 0 {
            return [0, 0, 0];
        }
        let mut acc = [0u64; 3];
        for px in self.rgba.as_chunks::<4>().0 {
            acc[0] += u64::from(px[0]);
            acc[1] += u64::from(px[1]);
            acc[2] += u64::from(px[2]);
        }
        #[allow(clippy::cast_possible_truncation)]
        [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8]
    }
}

/// Produces a downscaled still: the photo itself, or a representative frame
/// of a video.
pub trait StillDecoder: Send + Sync {
    /// Decodes and scales so that the longer edge is at most `max_edge`
    /// pixels. Never upscales. Applies the file's rotation.
    fn decode_scaled(&self, path: &Path, max_edge: u32) -> Result<DecodedImage>;
}

/// Target size that fits `(w, h)` into `max_edge` without upscaling.
#[must_use]
pub fn fit_within(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_edge || long == 0 {
        return (w, h);
    }
    let scale = f64::from(max_edge) / f64::from(long);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (s(w), s(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_within_never_upscales_and_keeps_ratio() {
        assert_eq!(fit_within(100, 50, 256), (100, 50));
        assert_eq!(fit_within(4000, 3000, 256), (256, 192));
        assert_eq!(fit_within(3000, 4000, 256), (192, 256));
        assert_eq!(fit_within(10_000, 1, 256), (256, 1));
        assert_eq!(fit_within(0, 0, 256), (0, 0));
    }

    #[test]
    fn jpeg_round_trip_and_average() {
        let img = DecodedImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        };
        assert_eq!(img.average_rgb(), [127, 0, 127]);
        let jpeg = img.to_jpeg(90).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (2, 1));
    }
}
