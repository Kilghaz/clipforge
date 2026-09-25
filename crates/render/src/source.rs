//! How the compositor gets pixels for a media item.

use std::sync::Arc;

use clipforge_core::MediaId;
use clipforge_media::{ColorTransfer, VideoFrame};

use crate::colour::{self, SourceTransfer};

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

/// A 16-bit HLG picture of an HDR video frame: BT.2020, full range,
/// interleaved RGBA (alpha opaque). Only the HDR render path asks for it.
#[derive(Clone, PartialEq, Eq)]
pub struct HlgImage {
    pub width: u32,
    pub height: u32,
    pub rgba16: Arc<Vec<u16>>,
}

impl std::fmt::Debug for HlgImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HlgImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

/// Supplies source pixels. Implemented by the app on top of the library
/// (thumbnails for preview, full decodes for export).
pub trait SourceProvider {
    /// A still for `media` whose longest edge is about `max_edge` pixels
    /// (larger is fine, smaller means upscaling). `None` while the image is
    /// not available yet; the compositor then draws a placeholder.
    fn still(&self, media: MediaId, max_edge: u32) -> Option<SourceImage>;

    /// The frame of video `media` at `source_time` (time within the file).
    /// Providers without video support fall back to the still.
    fn video_frame(
        &self,
        media: MediaId,
        source_time: clipforge_core::Ticks,
        max_edge: u32,
    ) -> Option<SourceImage> {
        let _ = source_time;
        self.still(media, max_edge)
    }

    /// For HDR output: the frame of an HDR video as 16-bit HLG. `None` for
    /// SDR sources (the compositor converts their 8-bit pictures itself)
    /// and for providers without HDR support.
    fn video_frame_hlg(
        &self,
        media: MediaId,
        source_time: clipforge_core::Ticks,
        max_edge: u32,
    ) -> Option<HlgImage> {
        let _ = (media, source_time, max_edge);
        None
    }
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

fn transfer_of(t: ColorTransfer) -> SourceTransfer {
    match t {
        ColorTransfer::Hlg => SourceTransfer::Hlg,
        ColorTransfer::Pq => SourceTransfer::Pq,
        ColorTransfer::Sdr | ColorTransfer::GainMap => SourceTransfer::Sdr,
    }
}

/// A decoded video frame as an SDR picture: SDR frames as they are, HDR
/// frames tone mapped (preview, SDR export).
#[must_use]
pub fn sdr_source(frame: VideoFrame) -> SourceImage {
    sdr_source_with(frame, colour::hdr16_to_sdr8_into)
}

/// [`sdr_source`] with the HDR → SDR conversion supplied by the caller
/// (same signature as [`colour::hdr16_to_sdr8_into`]), so crates that may
/// use threads can convert bands of a large frame in parallel.
pub fn sdr_source_with(
    frame: VideoFrame,
    convert: impl FnOnce(&[u16], SourceTransfer, &mut [u8]),
) -> SourceImage {
    match frame {
        VideoFrame::Sdr(img) => SourceImage {
            width: img.width,
            height: img.height,
            rgba: Arc::new(img.rgba),
        },
        VideoFrame::Hdr(img) => {
            let mut rgba = vec![0; img.rgb48.len() / 3 * 4];
            convert(&img.rgb48, transfer_of(img.transfer), &mut rgba);
            SourceImage {
                width: img.width,
                height: img.height,
                rgba: Arc::new(rgba),
            }
        }
    }
}

/// An HDR video frame as 16-bit HLG (HDR export); `None` for SDR frames,
/// which the compositor converts itself.
#[must_use]
pub fn hlg_source(frame: &VideoFrame) -> Option<HlgImage> {
    match frame {
        VideoFrame::Sdr(_) => None,
        VideoFrame::Hdr(img) => Some(HlgImage {
            width: img.width,
            height: img.height,
            rgba16: Arc::new(colour::hdr16_to_hlg16(
                &img.rgb48,
                transfer_of(img.transfer),
            )),
        }),
    }
}
