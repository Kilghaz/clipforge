//! Transition effects between two rendered frames.
//!
//! `apply(kind, from, to, progress)` is pure and works on whole frames, so
//! every effect can be tested with synthetic pictures. Movement effects
//! (slides, wipes, zoom) are eased; dissolves and fades are linear so the
//! perceived brightness changes evenly.

use clipforge_core::TransitionKind;

use crate::draw::{Filter, RectF, as_source, draw};
use crate::frame::Frame;

/// Smoothstep easing: slow start, slow end.
#[must_use]
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Colour a transition passes through when the first clip enters from
/// "nothing": white for the white fade, black for everything else.
#[must_use]
pub fn opening_colour(kind: TransitionKind) -> [u8; 3] {
    if kind == TransitionKind::FadeThroughWhite {
        [255, 255, 255]
    } else {
        [0, 0, 0]
    }
}

/// Blends from `from` to `to` at `progress` (0 = `from`, 1 = `to`). Both
/// frames must have the same size.
#[must_use]
pub fn apply(
    kind: TransitionKind,
    from: &Frame,
    to: &Frame,
    progress: f32,
    filter: Filter,
) -> Frame {
    let p = progress.clamp(0.0, 1.0);
    let (w, h) = (to.width, to.height);
    match kind {
        TransitionKind::Cut => to.clone(),
        TransitionKind::CrossDissolve => {
            let mut out = from.clone();
            out.blend_towards(to, p);
            out
        }
        TransitionKind::FadeThroughBlack | TransitionKind::FadeThroughWhite => {
            let colour = if kind == TransitionKind::FadeThroughWhite {
                [255, 255, 255]
            } else {
                [0, 0, 0]
            };
            let solid = Frame::solid(w, h, colour);
            if p < 0.5 {
                let mut out = from.clone();
                out.blend_towards(&solid, p * 2.0);
                out
            } else {
                let mut out = solid;
                out.blend_towards(to, (p - 0.5) * 2.0);
                out
            }
        }
        TransitionKind::SlideLeft
        | TransitionKind::SlideRight
        | TransitionKind::SlideUp
        | TransitionKind::SlideDown => {
            let e = f64::from(ease(p));
            let (fw, fh) = (i64::from(w), i64::from(h));
            #[allow(clippy::cast_possible_truncation)]
            let shift = |len: i64| (len as f64 * e).round() as i64;
            let (fx, fy, tx, ty) = match kind {
                TransitionKind::SlideLeft => (-shift(fw), 0, fw - shift(fw), 0),
                TransitionKind::SlideRight => (shift(fw), 0, shift(fw) - fw, 0),
                TransitionKind::SlideUp => (0, -shift(fh), 0, fh - shift(fh)),
                _ => (0, shift(fh), 0, shift(fh) - fh),
            };
            let mut out = Frame::black(w, h);
            out.blit(from, fx, fy);
            out.blit(to, tx, ty);
            out
        }
        TransitionKind::WipeLeft
        | TransitionKind::WipeRight
        | TransitionKind::WipeUp
        | TransitionKind::WipeDown => {
            let e = ease(p);
            let mut out = from.clone();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let edge = |len: u32| ((len as f32) * e).round() as u32;
            match kind {
                // Edge travels right-to-left: the incoming picture grows from the right.
                TransitionKind::WipeLeft => copy_columns(&mut out, to, w - edge(w), w),
                TransitionKind::WipeRight => copy_columns(&mut out, to, 0, edge(w)),
                TransitionKind::WipeUp => copy_rows(&mut out, to, h - edge(h), h),
                _ => copy_rows(&mut out, to, 0, edge(h)),
            }
            out
        }
        TransitionKind::Zoom => {
            // The incoming picture grows from 60 % to full size while fading in.
            let e = ease(p);
            let scale = 0.6 + 0.4 * f64::from(e);
            let full = RectF {
                x: 0.0,
                y: 0.0,
                width: f64::from(w),
                height: f64::from(h),
            };
            let mut grown = from.clone();
            draw(
                &mut grown,
                &as_source(to),
                full.zoomed(scale, 0.0, 0.0, w, h),
                filter,
            );
            let mut out = from.clone();
            out.blend_towards(&grown, e);
            out
        }
    }
}

fn copy_columns(dst: &mut Frame, src: &Frame, x0: u32, x1: u32) {
    if x1 <= x0 {
        return;
    }
    let (a, b) = ((x0 * 4) as usize, (x1 * 4) as usize);
    let stride = (dst.width * 4) as usize;
    for row in 0..dst.height as usize {
        let o = row * stride;
        dst.rgba[o + a..o + b].copy_from_slice(&src.rgba[o + a..o + b]);
    }
}

fn copy_rows(dst: &mut Frame, src: &Frame, y0: u32, y1: u32) {
    if y1 <= y0 {
        return;
    }
    let stride = (dst.width * 4) as usize;
    let (a, b) = (y0 as usize * stride, y1 as usize * stride);
    dst.rgba[a..b].copy_from_slice(&src.rgba[a..b]);
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 16;
    const H: u32 = 8;
    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    /// Outgoing frame: left half green, right half red, so slides (which
    /// move the picture) and wipes (which uncover in place) differ.
    fn from() -> Frame {
        let mut f = Frame::solid(W, H, RED);
        for y in 0..H {
            for x in 0..W / 2 {
                let i = ((y * W + x) * 4) as usize;
                f.rgba[i..i + 3].copy_from_slice(&GREEN);
            }
        }
        f
    }

    fn to() -> Frame {
        Frame::solid(W, H, BLUE)
    }

    fn at(kind: TransitionKind, p: f32) -> Frame {
        apply(kind, &from(), &to(), p, Filter::Fast)
    }

    fn rgb(f: &Frame, x: u32, y: u32) -> [u8; 3] {
        let p = f.pixel(x, y);
        [p[0], p[1], p[2]]
    }

    #[test]
    fn every_kind_starts_at_from_and_ends_at_to() {
        for kind in TransitionKind::ALL {
            if kind != TransitionKind::Cut {
                assert_eq!(at(kind, 0.0), from(), "{kind:?} at 0");
            }
            assert_eq!(at(kind, 1.0), to(), "{kind:?} at 1");
        }
    }

    #[test]
    fn dissolve_and_fades_at_half() {
        let d = at(TransitionKind::CrossDissolve, 0.5);
        let px = rgb(&d, 12, 4); // red over blue
        assert!(
            (120..=136).contains(&px[0]) && px[1] == 0 && (120..=136).contains(&px[2]),
            "{px:?}"
        );
        assert_eq!(
            rgb(&at(TransitionKind::FadeThroughBlack, 0.5), 3, 3),
            [0, 0, 0]
        );
        assert_eq!(
            rgb(&at(TransitionKind::FadeThroughWhite, 0.5), 3, 3),
            [255, 255, 255]
        );
        let q = rgb(&at(TransitionKind::FadeThroughWhite, 0.25), 12, 4);
        assert!(
            q[0] == 255 && (120..=136).contains(&q[1]),
            "red half way to white: {q:?}"
        );
    }

    #[test]
    fn slides_move_both_pictures() {
        // At half way the outgoing picture's right half (red) sits on the left.
        let l = at(TransitionKind::SlideLeft, 0.5);
        assert_eq!(rgb(&l, 2, 4), RED);
        assert_eq!(rgb(&l, 12, 4), BLUE);
        let r = at(TransitionKind::SlideRight, 0.5);
        assert_eq!(rgb(&r, 2, 4), BLUE);
        assert_eq!(rgb(&r, 12, 4), GREEN, "outgoing left half moved right");
        let u = at(TransitionKind::SlideUp, 0.5);
        assert_eq!(rgb(&u, 12, 1), RED);
        assert_eq!(rgb(&u, 12, 6), BLUE);
        let d = at(TransitionKind::SlideDown, 0.5);
        assert_eq!(rgb(&d, 12, 1), BLUE);
        assert_eq!(rgb(&d, 12, 6), RED);
    }

    #[test]
    fn wipes_uncover_in_place() {
        // WipeLeft at half: right half already blue, left half still the outgoing green.
        let l = at(TransitionKind::WipeLeft, 0.5);
        assert_eq!(rgb(&l, 2, 4), GREEN);
        assert_eq!(rgb(&l, 12, 4), BLUE);
        let r = at(TransitionKind::WipeRight, 0.5);
        assert_eq!(rgb(&r, 2, 4), BLUE);
        assert_eq!(rgb(&r, 12, 4), RED);
        let u = at(TransitionKind::WipeUp, 0.5);
        assert_eq!(rgb(&u, 2, 1), GREEN);
        assert_eq!(rgb(&u, 2, 6), BLUE);
        let d = at(TransitionKind::WipeDown, 0.5);
        assert_eq!(rgb(&d, 2, 1), BLUE);
        assert_eq!(rgb(&d, 2, 6), GREEN);
    }

    #[test]
    fn zoom_grows_from_the_centre() {
        let z = at(TransitionKind::Zoom, 0.5);
        // Corners keep the outgoing picture; the centre is blended towards blue.
        assert_eq!(rgb(&z, 0, 0), GREEN);
        assert_eq!(rgb(&z, W - 1, H - 1), RED);
        let c = rgb(&z, W / 2 + 1, H / 2);
        assert!(c[2] > 100 && c[0] < 160, "centre half way to blue: {c:?}");
    }

    #[test]
    fn easing_is_symmetric_and_bounded() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!((ease(0.5) - 0.5).abs() < 1e-6);
        assert!((ease(0.25) + ease(0.75) - 1.0).abs() < 1e-6);
        assert_eq!(ease(-3.0), 0.0);
        assert_eq!(ease(7.0), 1.0);
        assert_eq!(
            opening_colour(TransitionKind::FadeThroughWhite),
            [255, 255, 255]
        );
        assert_eq!(opening_colour(TransitionKind::SlideLeft), [0, 0, 0]);
    }
}
