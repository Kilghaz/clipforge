//! RGBA → planar YUV 4:2:0, BT.709, limited range. This is what we pipe to
//! ffmpeg: a quarter of the bytes of RGBA at 4K, and no colour conversion
//! left to the encoder's defaults.

use clipforge_render::Frame;

/// Planar I420 buffer for a `width × height` frame (both even).
#[derive(Clone, PartialEq, Eq)]
pub struct Yuv420 {
    pub width: u32,
    pub height: u32,
    /// Y plane, then U, then V, contiguous.
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Yuv420 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Yuv420")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl Yuv420 {
    /// Size in bytes of an I420 frame.
    #[must_use]
    pub fn byte_len(width: u32, height: u32) -> usize {
        let y = width as usize * height as usize;
        y + 2 * (width.div_ceil(2) as usize * height.div_ceil(2) as usize)
    }

    /// Converts an RGBA frame. Odd sizes are handled by clamping chroma
    /// sampling at the edge.
    #[must_use]
    pub fn from_frame(frame: &Frame) -> Yuv420 {
        let (w, h) = (frame.width as usize, frame.height as usize);
        let cw = w.div_ceil(2);
        let ch = h.div_ceil(2);
        let mut data = vec![0u8; w * h + 2 * cw * ch];
        let (y_plane, uv) = data.split_at_mut(w * h);
        let (u_plane, v_plane) = uv.split_at_mut(cw * ch);
        // Fixed-point BT.709 limited range (Y scaled by 219/255, chroma by
        // 224/255), 16.16 with rounding.
        for (i, px) in frame.rgba.as_chunks::<4>().0.iter().enumerate() {
            let (r, g, b) = (i32::from(px[0]), i32::from(px[1]), i32::from(px[2]));
            let y = ((11_966 * r + 40_254 * g + 4_064 * b + 32_768) >> 16) + 16;
            y_plane[i] = clamp8(y);
        }
        for cy in 0..ch {
            for cx in 0..cw {
                let (mut sr, mut sg, mut sb, mut n) = (0i32, 0i32, 0i32, 0i32);
                for dy in 0..2 {
                    for dx in 0..2 {
                        let x = (cx * 2 + dx).min(w - 1);
                        let y = (cy * 2 + dy).min(h - 1);
                        let o = (y * w + x) * 4;
                        sr += i32::from(frame.rgba[o]);
                        sg += i32::from(frame.rgba[o + 1]);
                        sb += i32::from(frame.rgba[o + 2]);
                        n += 1;
                    }
                }
                let (r, g, b) = (sr / n, sg / n, sb / n);
                let u = ((-6_598 * r - 22_186 * g + 28_784 * b + 32_768) >> 16) + 128;
                let v = ((28_784 * r - 26_149 * g - 2_636 * b + 32_768) >> 16) + 128;
                u_plane[cy * cw + cx] = clamp8(u);
                v_plane[cy * cw + cx] = clamp8(v);
            }
        }
        Yuv420 {
            width: frame.width,
            height: frame.height,
            data,
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamp8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yuv_of(rgb: [u8; 3]) -> (u8, u8, u8) {
        let y = Yuv420::from_frame(&Frame::solid(2, 2, rgb));
        (y.data[0], y.data[4], y.data[5])
    }

    #[test]
    fn limited_range_black_white_and_primaries() {
        assert_eq!(yuv_of([0, 0, 0]), (16, 128, 128));
        assert_eq!(yuv_of([255, 255, 255]), (235, 128, 128));
        let (y, u, v) = yuv_of([255, 0, 0]);
        assert!(
            (62..=64).contains(&y) && (100..=103).contains(&u) && (239..=241).contains(&v),
            "{y} {u} {v}"
        );
        let (y, u, v) = yuv_of([0, 0, 255]);
        assert!(
            (31..=33).contains(&y) && (239..=241).contains(&u) && (117..=119).contains(&v),
            "{y} {u} {v}"
        );
        let (y, ..) = yuv_of([128, 128, 128]);
        assert!((125..=127).contains(&y));
    }

    #[test]
    fn layout_and_sizes() {
        let f = Frame::solid(4, 2, [10, 20, 30]);
        let y = Yuv420::from_frame(&f);
        assert_eq!(y.data.len(), Yuv420::byte_len(4, 2));
        assert_eq!(y.data.len(), 8 + 2 * 2);
        assert_eq!(Yuv420::byte_len(1920, 1080), 1920 * 1080 * 3 / 2);
        // Odd sizes do not panic.
        let odd = Yuv420::from_frame(&Frame::solid(3, 3, [0, 0, 0]));
        assert_eq!(odd.data.len(), 9 + 2 * 4);
    }

    #[test]
    fn chroma_is_averaged_over_the_block() {
        let mut f = Frame::black(2, 2);
        f.rgba[0..4].copy_from_slice(&[255, 0, 0, 255]);
        let y = Yuv420::from_frame(&f);
        // One red pixel among three black: chroma roughly a quarter of the way to red.
        let v = y.data[5];
        assert!((150..=160).contains(&v), "{v}");
    }
}
