//! How the compositor gets pixels for a media item.

use std::sync::Arc;

use clipforge_core::MediaId;

/// A decoded still (or video frame), 8-bit RGBA, already rotated for
/// display according to the file's own metadata.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

impl std::fmt::Debug for SourceImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl SourceImage {
    #[must_use]
    pub fn solid(width: u32, height: u32, rgb: [u8; 3]) -> SourceImage {
        let mut rgba = vec![255u8; (width as usize) * (height as usize) * 4];
        for px in rgba.as_chunks_mut::<4>().0 {
            px[0] = rgb[0];
            px[1] = rgb[1];
            px[2] = rgb[2];
        }
        SourceImage {
            width,
            height,
            rgba: Arc::new(rgba),
        }
    }
}

/// Supplies source pixels. Implemented by the app on top of the library
/// (thumbnails for preview, full decodes for export).
pub trait SourceProvider {
    /// A still for `media` whose longest edge is about `max_edge` pixels
    /// (larger is fine, smaller means upscaling). `None` while the image is
    /// not available yet; the compositor then draws a placeholder.
    fn still(&self, media: MediaId, max_edge: u32) -> Option<SourceImage>;
}

/// A provider backed by a map, for tests.
#[derive(Debug, Default)]
pub struct MapProvider {
    pub images: std::collections::HashMap<MediaId, SourceImage>,
}

impl SourceProvider for MapProvider {
    fn still(&self, media: MediaId, _max_edge: u32) -> Option<SourceImage> {
        self.images.get(&media).cloned()
    }
}
