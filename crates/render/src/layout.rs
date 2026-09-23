//! Placing a picture into a frame.

use clipforge_core::Fit;

/// Integer rectangle in frame pixels. `x`/`y` may be negative for `Cover`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
}

/// Where a `src_w × src_h` picture goes in a `frame_w × frame_h` frame.
/// `Contain` letterboxes/pillarboxes; `Cover` fills and overflows evenly.
#[must_use]
pub fn place(src_w: u32, src_h: u32, frame_w: u32, frame_h: u32, fit: Fit) -> Rect {
    if src_w == 0 || src_h == 0 || frame_w == 0 || frame_h == 0 {
        return Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }
    let sx = f64::from(frame_w) / f64::from(src_w);
    let sy = f64::from(frame_h) / f64::from(src_h);
    let scale = match fit {
        Fit::Contain => sx.min(sy),
        Fit::Cover => sx.max(sy),
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut width = (f64::from(src_w) * scale).round().max(1.0) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut height = (f64::from(src_h) * scale).round().max(1.0) as u32;
    // Snap the fitted edge exactly to the frame to avoid 1px slivers.
    match fit {
        Fit::Contain => {
            if width > frame_w {
                width = frame_w;
            }
            if height > frame_h {
                height = frame_h;
            }
        }
        Fit::Cover => {
            if width < frame_w {
                width = frame_w;
            }
            if height < frame_h {
                height = frame_h;
            }
        }
    }
    let x = (i64::from(frame_w) - i64::from(width)) / 2;
    let y = (i64::from(frame_h) - i64::from(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contain_letterboxes_wide_and_pillarboxes_tall() {
        // 4:3 photo into 16:9 frame: full height, bars left/right.
        let r = place(4000, 3000, 1920, 1080, Fit::Contain);
        assert_eq!(
            r,
            Rect {
                x: 240,
                y: 0,
                width: 1440,
                height: 1080
            }
        );
        // Portrait into landscape.
        let r = place(3000, 4000, 1920, 1080, Fit::Contain);
        assert_eq!((r.width, r.height, r.y), (810, 1080, 0));
        assert_eq!(r.x, (1920 - 810) / 2);
        // Ultra-wide into 16:9: full width, bars top/bottom.
        let r = place(3000, 1000, 1920, 1080, Fit::Contain);
        assert_eq!((r.width, r.height, r.x), (1920, 640, 0));
        assert_eq!(r.y, 220);
    }

    #[test]
    fn cover_fills_and_centres_overflow() {
        let r = place(4000, 3000, 1920, 1080, Fit::Cover);
        assert_eq!((r.width, r.height), (1920, 1440));
        assert_eq!(r.y, -180);
        assert_eq!(r.x, 0);
        let r = place(3000, 4000, 1920, 1080, Fit::Cover);
        assert_eq!((r.width, r.height, r.y), (1920, 2560, -740));
    }

    #[test]
    fn same_aspect_is_exact_and_small_sources_scale_up() {
        assert_eq!(
            place(1920, 1080, 960, 540, Fit::Contain),
            Rect {
                x: 0,
                y: 0,
                width: 960,
                height: 540
            }
        );
        assert_eq!(
            place(16, 9, 1920, 1080, Fit::Cover),
            Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(place(0, 10, 100, 100, Fit::Contain).width, 0);
    }
}
