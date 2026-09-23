//! A rendered frame: tightly packed 8-bit sRGB RGBA.

#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl Frame {
    /// An opaque black frame.
    #[must_use]
    pub fn black(width: u32, height: u32) -> Frame {
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        Frame {
            width,
            height,
            rgba,
        }
    }

    /// A solid colour frame (tests, placeholders).
    #[must_use]
    pub fn solid(width: u32, height: u32, rgb: [u8; 3]) -> Frame {
        let mut f = Frame::black(width, height);
        for px in f.rgba.as_chunks_mut::<4>().0 {
            px[0] = rgb[0];
            px[1] = rgb[1];
            px[2] = rgb[2];
        }
        f
    }

    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// Copies `src` onto `self` at (`x`, `y`), clipping to the frame.
    pub fn blit(&mut self, src: &Frame, x: i64, y: i64) {
        let fw = i64::from(self.width);
        let fh = i64::from(self.height);
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + i64::from(src.width)).min(fw);
        let y1 = (y + i64::from(src.height)).min(fh);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let row_len = ((x1 - x0) * 4) as usize;
        for row in y0..y1 {
            let src_row = (row - y) as usize;
            let src_col = (x0 - x) as usize;
            let src_off = (src_row * src.width as usize + src_col) * 4;
            let dst_off = (row as usize * self.width as usize + x0 as usize) * 4;
            self.rgba[dst_off..dst_off + row_len]
                .copy_from_slice(&src.rgba[src_off..src_off + row_len]);
        }
    }

    /// Linear blend towards `other`: `t = 0` keeps `self`, `t = 1` is `other`.
    /// Both frames must have the same size.
    pub fn blend_towards(&mut self, other: &Frame, t: f32) {
        debug_assert_eq!((self.width, self.height), (other.width, other.height));
        let t = t.clamp(0.0, 1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let w = (t * 256.0).round() as u32;
        for (dst, src) in self.rgba.iter_mut().zip(&other.rgba) {
            *dst = ((u32::from(*dst) * (256 - w) + u32::from(*src) * w) >> 8) as u8;
        }
    }

    /// Multiplies RGB by `factor` in `0..=1` (fade to black).
    pub fn darken(&mut self, factor: f32) {
        let factor = factor.clamp(0.0, 1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let w = (factor * 256.0).round() as u32;
        for px in self.rgba.as_chunks_mut::<4>().0 {
            px[0] = ((u32::from(px[0]) * w) >> 8) as u8;
            px[1] = ((u32::from(px[1]) * w) >> 8) as u8;
            px[2] = ((u32::from(px[2]) * w) >> 8) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_and_solid() {
        let f = Frame::black(2, 2);
        assert_eq!(f.pixel(1, 1), [0, 0, 0, 255]);
        let s = Frame::solid(2, 2, [10, 20, 30]);
        assert_eq!(s.pixel(0, 1), [10, 20, 30, 255]);
    }

    #[test]
    fn blit_clips_at_all_edges() {
        let mut dst = Frame::black(4, 4);
        let src = Frame::solid(3, 3, [255, 0, 0]);
        dst.blit(&src, -1, -1);
        assert_eq!(dst.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(dst.pixel(1, 1), [255, 0, 0, 255]);
        assert_eq!(dst.pixel(2, 2), [0, 0, 0, 255]);
        let mut dst = Frame::black(4, 4);
        dst.blit(&src, 3, 3);
        assert_eq!(dst.pixel(3, 3), [255, 0, 0, 255]);
        assert_eq!(dst.pixel(2, 2), [0, 0, 0, 255]);
        let mut dst = Frame::black(4, 4);
        dst.blit(&src, 10, 10);
        assert_eq!(dst, Frame::black(4, 4));
    }

    #[test]
    fn blend_and_darken_endpoints_are_exact() {
        let a = Frame::solid(1, 1, [200, 100, 0]);
        let b = Frame::solid(1, 1, [0, 100, 200]);
        let mut x = a.clone();
        x.blend_towards(&b, 0.0);
        assert_eq!(x, a);
        let mut x = a.clone();
        x.blend_towards(&b, 1.0);
        assert_eq!(x.pixel(0, 0)[..3], [0, 100, 200]);
        let mut x = a.clone();
        x.blend_towards(&b, 0.5);
        let p = x.pixel(0, 0);
        assert!((99..=101).contains(&p[0]) && p[1] == 100 && (99..=101).contains(&p[2]));
        let mut d = a.clone();
        d.darken(0.0);
        assert_eq!(d.pixel(0, 0), [0, 0, 0, 255]);
        let mut d = a.clone();
        d.darken(1.0);
        assert_eq!(d, a);
    }
}
