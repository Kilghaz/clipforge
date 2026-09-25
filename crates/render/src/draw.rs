//! Drawing a picture into an arbitrary rectangle of a frame.
//!
//! Fit, cover, Ken Burns and the zoom transition are all "put this picture
//! at this (possibly larger than the frame, possibly off-screen) rectangle".
//! Only the visible part of the source is resampled, so zooming into a
//! large photo costs the same as drawing it at frame size.

use fast_image_resize as fr;

use crate::frame::Frame;
use crate::source::SourceImage;

/// Rectangle in frame pixels with fractional precision.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RectF {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RectF {
    /// Scales the rectangle by `factor` around the centre of a `w × h`
    /// frame and moves it by (`dx`, `dy`) pixels.
    #[must_use]
    pub fn zoomed(self, factor: f64, dx: f64, dy: f64, frame_w: u32, frame_h: u32) -> RectF {
        let cx = f64::from(frame_w) / 2.0;
        let cy = f64::from(frame_h) / 2.0;
        RectF {
            x: cx + (self.x - cx) * factor + dx,
            y: cy + (self.y - cy) * factor + dy,
            width: self.width * factor,
            height: self.height * factor,
        }
    }
}

/// Resampling quality.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Filter {
    /// Bilinear: fast, used for the preview.
    Fast,
    /// Lanczos3: sharp, used for export.
    Sharp,
}

impl Filter {
    fn fr(self) -> fr::FilterType {
        match self {
            Filter::Fast => fr::FilterType::Bilinear,
            Filter::Sharp => fr::FilterType::Lanczos3,
        }
    }
}

/// Draws `src` so that it covers `rect` in `frame`, clipped to the frame.
/// Pixels of the frame outside `rect` are left untouched.
pub fn draw(frame: &mut Frame, src: &SourceImage, rect: RectF, filter: Filter) {
    if src.width == 0 || src.height == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let fw = f64::from(frame.width);
    let fh = f64::from(frame.height);
    // Visible destination region, snapped to whole pixels.
    let x0 = rect.x.max(0.0).round();
    let y0 = rect.y.max(0.0).round();
    let x1 = (rect.x + rect.width).min(fw).round();
    let y1 = (rect.y + rect.height).min(fh).round();
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (dw, dh) = ((x1 - x0) as u32, (y1 - y0) as u32);
    // Matching source region.
    let sx = f64::from(src.width) / rect.width;
    let sy = f64::from(src.height) / rect.height;
    let crop_x = ((x0 - rect.x) * sx).clamp(0.0, f64::from(src.width));
    let crop_y = ((y0 - rect.y) * sy).clamp(0.0, f64::from(src.height));
    let crop_w = (f64::from(dw) * sx).min(f64::from(src.width) - crop_x);
    let crop_h = (f64::from(dh) * sy).min(f64::from(src.height) - crop_y);
    if crop_w <= 0.0 || crop_h <= 0.0 {
        return;
    }
    // Exact 1:1 copy needs no resampler.
    let one_to_one = (crop_w - f64::from(dw)).abs() < 1e-9 && (crop_h - f64::from(dh)).abs() < 1e-9;
    let scaled = if one_to_one && crop_x.fract() == 0.0 && crop_y.fract() == 0.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        copy_region(src, crop_x as u32, crop_y as u32, dw, dh)
    } else {
        match resample(src, (crop_x, crop_y, crop_w, crop_h), dw, dh, filter) {
            Some(f) => f,
            None => return,
        }
    };
    #[allow(clippy::cast_possible_truncation)]
    frame.blit(&scaled, x0 as i64, y0 as i64);
}

fn copy_region(src: &SourceImage, x: u32, y: u32, w: u32, h: u32) -> Frame {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * src.width + x) * 4) as usize;
        rgba.extend_from_slice(&src.rgba[start..start + (w * 4) as usize]);
    }
    Frame {
        width: w,
        height: h,
        rgba,
    }
}

fn resample(
    src: &SourceImage,
    crop: (f64, f64, f64, f64),
    w: u32,
    h: u32,
    filter: Filter,
) -> Option<Frame> {
    let src_img =
        fr::images::ImageRef::new(src.width, src.height, &src.rgba, fr::PixelType::U8x4).ok()?;
    let mut dst = fr::images::Image::new(w, h, fr::PixelType::U8x4);
    let options = fr::ResizeOptions::new()
        .resize_alg(fr::ResizeAlg::Convolution(filter.fr()))
        .crop(crop.0, crop.1, crop.2, crop.3);
    fr::Resizer::new()
        .resize(&src_img, &mut dst, &options)
        .ok()?;
    let mut rgba = dst.into_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    Some(Frame {
        width: w,
        height: h,
        rgba,
    })
}

/// Wraps a frame as a source image (for drawing a rendered frame scaled).
#[must_use]
pub fn as_source(frame: &Frame) -> SourceImage {
    SourceImage {
        width: frame.width,
        height: frame.height,
        rgba: std::sync::Arc::new(frame.rgba.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4 × 2 source: left half red, right half blue.
    fn halves() -> SourceImage {
        let mut rgba = Vec::new();
        for _y in 0..2 {
            for x in 0..4 {
                rgba.extend_from_slice(if x < 2 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        SourceImage {
            width: 4,
            height: 2,
            rgba: std::sync::Arc::new(rgba),
        }
    }

    fn rgb(f: &Frame, x: u32, y: u32) -> [u8; 3] {
        let p = f.pixel(x, y);
        [p[0], p[1], p[2]]
    }

    #[test]
    fn one_to_one_draw_copies_exactly() {
        let mut f = Frame::black(4, 2);
        draw(
            &mut f,
            &halves(),
            RectF {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 2.0,
            },
            Filter::Sharp,
        );
        assert_eq!(rgb(&f, 0, 0), [255, 0, 0]);
        assert_eq!(rgb(&f, 3, 1), [0, 0, 255]);
    }

    #[test]
    fn offscreen_parts_are_clipped_and_the_rest_untouched() {
        let mut f = Frame::solid(8, 4, [0, 255, 0]);
        // Double size, shifted left by 4 px: only the red half's right part and the blue half show.
        draw(
            &mut f,
            &halves(),
            RectF {
                x: -4.0,
                y: 0.0,
                width: 8.0,
                height: 4.0,
            },
            Filter::Fast,
        );
        assert_eq!(
            rgb(&f, 2, 0),
            [0, 0, 255],
            "the blue half now fills the left"
        );
        assert_eq!(rgb(&f, 5, 2), [0, 255, 0], "outside the rect stays green");
        let mut g = Frame::solid(8, 4, [0, 255, 0]);
        draw(
            &mut g,
            &halves(),
            RectF {
                x: 20.0,
                y: 0.0,
                width: 8.0,
                height: 4.0,
            },
            Filter::Fast,
        );
        assert_eq!(
            g,
            Frame::solid(8, 4, [0, 255, 0]),
            "fully off-screen draws nothing"
        );
    }

    #[test]
    fn zoomed_scales_around_the_frame_centre() {
        let r = RectF {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        let z = r.zoomed(2.0, 0.0, 0.0, 100, 50);
        assert_eq!(
            z,
            RectF {
                x: -50.0,
                y: -25.0,
                width: 200.0,
                height: 100.0
            }
        );
        let m = r.zoomed(1.0, 10.0, -5.0, 100, 50);
        assert_eq!((m.x, m.y), (10.0, -5.0));
    }

    #[test]
    fn degenerate_inputs_are_ignored() {
        let mut f = Frame::black(2, 2);
        draw(
            &mut f,
            &halves(),
            RectF {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 2.0,
            },
            Filter::Fast,
        );
        assert_eq!(f, Frame::black(2, 2));
        let empty = SourceImage {
            width: 0,
            height: 0,
            rgba: std::sync::Arc::new(Vec::new()),
        };
        draw(
            &mut f,
            &empty,
            RectF {
                x: 0.0,
                y: 0.0,
                width: 2.0,
                height: 2.0,
            },
            Filter::Fast,
        );
        assert_eq!(f, Frame::black(2, 2));
    }
}
