//! Colour maths for the HDR path (ADR-0005): transfer functions, the
//! BT.2020 ↔ BT.709 gamut conversion, SDR content inside HLG (BT.2408) and
//! the tone mapping of HDR sources for SDR output.
//!
//! Conventions: "signal" values are non-linear (what files store), 0..1;
//! "scene" / "display" values are linear light. HLG display light is in
//! units of the nominal peak (1000 cd/m²); SDR reference white sits at
//! 203 cd/m² (BT.2408), i.e. HLG signal 0.75.

use std::sync::OnceLock;

/// HLG nominal peak luminance (cd/m²) for the display-referred conversion.
pub const HLG_PEAK_NITS: f32 = 1000.0;
/// Reference ("graphics") white in HDR, BT.2408.
pub const REFERENCE_WHITE_NITS: f32 = 203.0;
/// PQ absolute maximum.
pub const PQ_MAX_NITS: f32 = 10_000.0;

const HLG_A: f32 = 0.178_832_77;
const HLG_B: f32 = 0.284_668_92; // 1 - 4a
const HLG_C: f32 = 0.559_910_7; // 0.5 - a ln(4a)
/// System gamma of the HLG OOTF at 1000 cd/m².
const HLG_GAMMA: f32 = 1.2;

/// BT.2100 HLG OETF: scene linear (0..1) → signal.
#[must_use]
pub fn hlg_oetf(e: f32) -> f32 {
    let e = e.max(0.0);
    if e <= 1.0 / 12.0 {
        (3.0 * e).sqrt()
    } else {
        HLG_A * (12.0 * e - HLG_B).ln() + HLG_C
    }
}

/// Inverse of [`hlg_oetf`]: signal → scene linear (0..1).
#[must_use]
pub fn hlg_inverse_oetf(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.5 {
        v * v / 3.0
    } else {
        (((v - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    }
}

/// BT.2100 PQ EOTF: signal → display light as a fraction of 10 000 cd/m².
#[must_use]
pub fn pq_eotf(v: f32) -> f32 {
    const M1: f32 = 0.159_301_76;
    const M2: f32 = 78.843_75;
    const C1: f32 = 0.835_937_5;
    const C2: f32 = 18.851_563;
    const C3: f32 = 18.6875;
    let p = v.clamp(0.0, 1.0).powf(1.0 / M2);
    ((p - C1).max(0.0) / (C2 - C3 * p)).powf(1.0 / M1)
}

/// BT.709 / sRGB-ish SDR decoding used for photos (sRGB curve).
#[must_use]
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Inverse of [`srgb_to_linear`].
#[must_use]
pub fn linear_to_srgb(l: f32) -> f32 {
    let l = l.max(0.0);
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Linear BT.2020 RGB → linear BT.709 RGB (may leave 0..1).
#[must_use]
pub fn bt2020_to_bt709(c: [f32; 3]) -> [f32; 3] {
    [
        1.660_491 * c[0] - 0.587_641 * c[1] - 0.072_850 * c[2],
        -0.124_550 * c[0] + 1.132_9 * c[1] - 0.008_349 * c[2],
        -0.018_151 * c[0] - 0.100_579 * c[1] + 1.118_73 * c[2],
    ]
}

/// Linear BT.709 RGB → linear BT.2020 RGB.
#[must_use]
pub fn bt709_to_bt2020(c: [f32; 3]) -> [f32; 3] {
    [
        0.627_404 * c[0] + 0.329_283 * c[1] + 0.043_313 * c[2],
        0.069_097 * c[0] + 0.919_540 * c[1] + 0.011_362 * c[2],
        0.016_391 * c[0] + 0.088_013 * c[1] + 0.895_595 * c[2],
    ]
}

/// BT.2020 luminance weights.
fn luma2020(c: [f32; 3]) -> f32 {
    0.2627 * c[0] + 0.6780 * c[1] + 0.0593 * c[2]
}

/// HLG signal (BT.2020 R'G'B') → display light in units of the 1000 cd/m²
/// peak (the BT.2100 OOTF with γ = 1.2).
#[must_use]
pub fn hlg_signal_to_display(rgb: [f32; 3]) -> [f32; 3] {
    let scene = rgb.map(hlg_inverse_oetf);
    let ys = luma2020(scene);
    let gain = if ys > 0.0 {
        ys.powf(HLG_GAMMA - 1.0)
    } else {
        0.0
    };
    scene.map(|e| e * gain)
}

/// Display light (units of the 1000 cd/m² peak, BT.2020) → HLG signal
/// (inverse OOTF, then OETF).
#[must_use]
pub fn display_to_hlg_signal(display: [f32; 3]) -> [f32; 3] {
    let yd = luma2020(display).max(0.0);
    let scene = if yd > 0.0 {
        let gain = yd.powf((1.0 - HLG_GAMMA) / HLG_GAMMA);
        display.map(|d| d.max(0.0) * gain)
    } else {
        [0.0; 3]
    };
    scene.map(hlg_oetf)
}

/// An SDR pixel (sRGB-coded BT.709, 0..1) inside an HLG picture: SDR white
/// lands on the BT.2408 reference white (HLG signal 0.75), colours keep
/// their appearance ("SDR content is not brightened", ADR-0005).
#[must_use]
pub fn sdr_to_hlg(rgb: [f32; 3]) -> [f32; 3] {
    let linear709 = rgb.map(srgb_to_linear);
    let linear2020 = bt709_to_bt2020(linear709);
    let scale = REFERENCE_WHITE_NITS / HLG_PEAK_NITS;
    display_to_hlg_signal(linear2020.map(|c| c * scale))
}

/// BT.2390-style soft knee: identity up to `knee`, then compresses
/// everything above into `knee..max` smoothly. Values are relative to SDR
/// reference white (1.0 = 203 cd/m²).
fn roll_off(x: f32, knee: f32, max_in: f32) -> f32 {
    if x <= knee {
        return x;
    }
    let max_out = 1.0;
    // Hermite curve from (knee, knee) with slope 1 to (max_in, max_out)
    // with slope 0.
    let t = ((x - knee) / (max_in - knee)).clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;
    let span = max_in - knee;
    let p0 = knee;
    let m0 = span; // slope 1 scaled by the span
    let p1 = max_out;
    (2.0 * t3 - 3.0 * t2 + 1.0) * p0 + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * p1
}

/// HDR display light relative to SDR white (BT.2020 linear) → an sRGB-coded
/// BT.709 SDR pixel. Highlights above ~80 % of SDR white roll off softly
/// into white instead of clipping; the tone curve works on luminance so
/// hues stay put.
#[must_use]
pub fn tone_map_to_sdr(relative2020: [f32; 3], peak_relative: f32) -> [f32; 3] {
    tone_map_linear(relative2020, peak_relative).map(linear_to_srgb)
}

/// [`tone_map_to_sdr`] without the final sRGB encoding (linear BT.709
/// 0..1 out), so frame conversions can encode through a table.
fn tone_map_linear(relative2020: [f32; 3], peak_relative: f32) -> [f32; 3] {
    let y = luma2020(relative2020).max(0.0);
    let knee = 0.8;
    let mapped_y = roll_off(y, knee, peak_relative.max(knee + 0.01));
    let scale = if y > 0.0 { mapped_y / y } else { 0.0 };
    let linear709 = bt2020_to_bt709(relative2020.map(|c| c * scale));
    // Out-of-gamut and above-white parts desaturate towards the luminance.
    let max = linear709.iter().copied().fold(0.0f32, f32::max);
    let linear709 = if max > 1.0 {
        let l = mapped_y.min(1.0);
        let k = ((1.0 - l) / (max - l)).clamp(0.0, 1.0);
        linear709.map(|c| l + (c - l) * k)
    } else {
        linear709
    };
    linear709.map(|c| c.clamp(0.0, 1.0))
}

/// An HLG pixel (signal, BT.2020) as it should look on an SDR screen.
#[must_use]
pub fn hlg_to_sdr(rgb: [f32; 3]) -> [f32; 3] {
    let display = hlg_signal_to_display(rgb);
    let rel = HLG_PEAK_NITS / REFERENCE_WHITE_NITS;
    tone_map_to_sdr(display.map(|d| d * rel), rel)
}

/// A PQ pixel (signal, BT.2020) as it should look on an SDR screen.
#[must_use]
pub fn pq_to_sdr(rgb: [f32; 3]) -> [f32; 3] {
    let display = rgb.map(pq_eotf);
    let rel = PQ_MAX_NITS / REFERENCE_WHITE_NITS;
    // Consumer PQ masters rarely exceed 1000 cd/m²; roll off towards that.
    tone_map_to_sdr(
        display.map(|d| d * rel),
        HLG_PEAK_NITS / REFERENCE_WHITE_NITS,
    )
}

/// A PQ pixel converted to HLG (for HDR export of PQ sources).
#[must_use]
pub fn pq_to_hlg(rgb: [f32; 3]) -> [f32; 3] {
    let nits = rgb.map(|v| pq_eotf(v) * PQ_MAX_NITS);
    display_to_hlg_signal(nits.map(|n| (n / HLG_PEAK_NITS).min(1.0)))
}

/// How a source's pixels are encoded.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum SourceTransfer {
    /// sRGB / BT.709, 8-bit.
    #[default]
    Sdr,
    /// BT.2020 HLG.
    Hlg,
    /// BT.2020 PQ.
    Pq,
}

/// Converts 16-bit RGB samples of an HDR source to 8-bit sRGB for SDR
/// output (preview, SDR export). `rgb48` is interleaved R, G, B (u16).
/// Returns interleaved RGBA8.
#[must_use]
pub fn hdr16_to_sdr8(rgb48: &[u16], transfer: SourceTransfer) -> Vec<u8> {
    let mut out = vec![0; rgb48.len() / 3 * 4];
    hdr16_to_sdr8_into(rgb48, transfer, &mut out);
    out
}

/// [`hdr16_to_sdr8`] into a caller-provided buffer (4 bytes per 3 input
/// samples), so callers that may use threads can convert bands in
/// parallel.
pub fn hdr16_to_sdr8_into(rgb48: &[u16], transfer: SourceTransfer, out: &mut [u8]) {
    let lut = signal_lut(transfer);
    for (px, o) in rgb48
        .as_chunks::<3>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<4>().0)
    {
        let sdr = match transfer {
            SourceTransfer::Sdr => [px[0], px[1], px[2]].map(|v| f32::from(v) / 65_535.0),
            SourceTransfer::Hlg | SourceTransfer::Pq => lut.map_pixel([px[0], px[1], px[2]]),
        };
        let [r, g, b] = sdr.map(to_u8);
        o.copy_from_slice(&[r, g, b, 255]);
    }
}

/// Converts 16-bit RGB samples of an HDR source to 16-bit HLG RGBA for HDR
/// output. HLG passes through; PQ is converted.
#[must_use]
pub fn hdr16_to_hlg16(rgb48: &[u16], transfer: SourceTransfer) -> Vec<u16> {
    let mut out = Vec::with_capacity(rgb48.len() / 3 * 4);
    for px in rgb48.as_chunks::<3>().0 {
        match transfer {
            SourceTransfer::Hlg => out.extend_from_slice(&[px[0], px[1], px[2]]),
            SourceTransfer::Pq => {
                let hlg = pq_to_hlg([px[0], px[1], px[2]].map(|v| f32::from(v) / 65_535.0));
                out.extend(hlg.map(to_u16));
            }
            SourceTransfer::Sdr => {
                let hlg = sdr_to_hlg([px[0], px[1], px[2]].map(|v| f32::from(v) / 65_535.0));
                out.extend(hlg.map(to_u16));
            }
        }
        out.push(u16::MAX);
    }
    out
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 65_535.0).round() as u16
}

/// Speeds up HDR → SDR: the transfer function is sampled once per 16-bit
/// code value (12-bit table, interpolated) and the rest is a few
/// multiplications per pixel.
struct SignalLut {
    transfer: SourceTransfer,
    /// Linear light relative to SDR reference white, per 12-bit code.
    to_linear: Vec<f32>,
    /// `y^(γ-1)` for the HLG OOTF, indexed by `sqrt(y)` (fine where the
    /// curve is steep, near black).
    gain: Vec<f32>,
    /// sRGB encoding of linear 0..1, indexed by `sqrt(l)`.
    encode: Vec<f32>,
}

/// Samples of `f(t²)` for t in 0..=1: a table indexed by `sqrt(x)`.
#[allow(clippy::cast_precision_loss)]
fn sqrt_table(n: usize, f: impl Fn(f32) -> f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            f(t * t)
        })
        .collect()
}

/// Looks `x` (0..1) up in a [`sqrt_table`], interpolated.
#[allow(clippy::cast_precision_loss)]
fn sqrt_lookup(table: &[f32], x: f32) -> f32 {
    let pos = x.clamp(0.0, 1.0).sqrt() * (table.len() - 1) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = pos as usize;
    let j = (i + 1).min(table.len() - 1);
    let f = pos - i as f32;
    table[i] + (table[j] - table[i]) * f
}

/// The tables for `transfer`, built once per process.
fn signal_lut(transfer: SourceTransfer) -> &'static SignalLut {
    static TABLES: [OnceLock<SignalLut>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = match transfer {
        SourceTransfer::Sdr => 0,
        SourceTransfer::Hlg => 1,
        SourceTransfer::Pq => 2,
    };
    TABLES[slot].get_or_init(|| build_signal_lut(transfer))
}

fn build_signal_lut(transfer: SourceTransfer) -> SignalLut {
    const N: usize = 4096;
    #[allow(clippy::cast_precision_loss)]
    let to_linear = (0..N)
        .map(|i| {
            let v = i as f32 / (N - 1) as f32;
            match transfer {
                SourceTransfer::Hlg => hlg_inverse_oetf(v),
                SourceTransfer::Pq => pq_eotf(v) * PQ_MAX_NITS / REFERENCE_WHITE_NITS,
                SourceTransfer::Sdr => srgb_to_linear(v),
            }
        })
        .collect();
    SignalLut {
        transfer,
        to_linear,
        gain: sqrt_table(4096, |y| {
            if y > 0.0 {
                y.powf(HLG_GAMMA - 1.0)
            } else {
                0.0
            }
        }),
        encode: sqrt_table(4096, linear_to_srgb),
    }
}

impl SignalLut {
    #[allow(clippy::cast_precision_loss)]
    fn lookup(&self, code: u16) -> f32 {
        let pos = f32::from(code) / 65_535.0 * (self.to_linear.len() - 1) as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = pos.floor() as usize;
        let j = (i + 1).min(self.to_linear.len() - 1);
        let f = pos - pos.floor();
        self.to_linear[i] + (self.to_linear[j] - self.to_linear[i]) * f
    }

    fn map_pixel(&self, px: [u16; 3]) -> [f32; 3] {
        let lin = px.map(|c| self.lookup(c));
        let rel = HLG_PEAK_NITS / REFERENCE_WHITE_NITS;
        let display = match self.transfer {
            SourceTransfer::Hlg => {
                // OOTF on the scene light, then relative to SDR white.
                let gain = sqrt_lookup(&self.gain, luma2020(lin));
                lin.map(|e| e * gain * rel)
            }
            SourceTransfer::Pq => lin,
            SourceTransfer::Sdr => return lin.map(|l| sqrt_lookup(&self.encode, l)),
        };
        tone_map_linear(display, rel).map(|l| sqrt_lookup(&self.encode, l))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn hlg_curve_matches_bt2100_reference_points() {
        assert!(close(hlg_oetf(0.0), 0.0, 1e-6));
        assert!(close(hlg_oetf(1.0 / 12.0), 0.5, 1e-5));
        assert!(close(hlg_oetf(1.0), 1.0, 1e-4));
        for i in 0..=100 {
            #[allow(clippy::cast_precision_loss)]
            let v = i as f32 / 100.0;
            assert!(close(hlg_oetf(hlg_inverse_oetf(v)), v, 1e-4), "{v}");
        }
    }

    #[test]
    fn pq_curve_hits_its_reference_luminances() {
        // PQ signal 0.58 ≈ 203 cd/m², 0.75 ≈ 1000 cd/m², 1.0 = 10 000.
        assert!(close(pq_eotf(1.0) * PQ_MAX_NITS, 10_000.0, 1.0));
        assert!(
            close(pq_eotf(0.58) * PQ_MAX_NITS, 203.0, 10.0),
            "{}",
            pq_eotf(0.58) * PQ_MAX_NITS
        );
        assert!(close(pq_eotf(0.7518) * PQ_MAX_NITS, 1000.0, 20.0));
        assert_eq!(pq_eotf(0.0), 0.0);
    }

    #[test]
    fn sdr_white_lands_on_the_hlg_reference_white() {
        // BT.2408: SDR / graphics white at HLG signal 75 %.
        let w = sdr_to_hlg([1.0, 1.0, 1.0]);
        for c in w {
            assert!(close(c, 0.75, 0.01), "{w:?}");
        }
        let black = sdr_to_hlg([0.0, 0.0, 0.0]);
        assert!(black.iter().all(|c| *c < 1e-4));
        // Saturated SDR red stays inside BT.2020, below white.
        let red = sdr_to_hlg([1.0, 0.0, 0.0]);
        assert!(
            red[0] > red[1] && red[0] > red[2] && red[0] < 0.8,
            "{red:?}"
        );
    }

    #[test]
    fn gamut_matrices_round_trip_and_keep_white() {
        let white = bt709_to_bt2020([1.0, 1.0, 1.0]);
        assert!(white.iter().all(|c| close(*c, 1.0, 1e-3)), "{white:?}");
        let c = [0.2, 0.5, 0.8];
        let back = bt2020_to_bt709(bt709_to_bt2020(c));
        assert!(
            back.iter().zip(c).all(|(a, b)| close(*a, b, 1e-3)),
            "{back:?}"
        );
    }

    #[test]
    fn hlg_reference_white_looks_white_in_sdr_and_highlights_roll_off() {
        let w = hlg_to_sdr([0.75, 0.75, 0.75]);
        assert!(
            w.iter().all(|c| *c > 0.93),
            "reference white stays near white: {w:?}"
        );
        let peak = hlg_to_sdr([1.0, 1.0, 1.0]);
        assert!(peak.iter().all(|c| close(*c, 1.0, 1e-3)), "{peak:?}");
        let mid = hlg_to_sdr([0.5, 0.5, 0.5]);
        assert!(
            mid.iter().all(|c| *c > 0.3 && *c < 0.75),
            "mid grey is mid grey: {mid:?}"
        );
        let black = hlg_to_sdr([0.0, 0.0, 0.0]);
        assert!(black.iter().all(|c| *c < 1e-3));
        // Monotonic along the grey axis: no banding reversal.
        let mut last = -1.0;
        for i in 0..=50 {
            #[allow(clippy::cast_precision_loss)]
            let v = hlg_to_sdr([i as f32 / 50.0; 3])[1];
            assert!(v >= last - 1e-5, "{i}: {v} < {last}");
            last = v;
        }
    }

    #[test]
    fn an_sdr_picture_survives_the_trip_through_hlg() {
        // SDR → HLG (HDR export) → SDR (what an SDR screen shows).
        for c in [
            [0.2, 0.4, 0.6],
            [0.5, 0.5, 0.5],
            [0.9, 0.1, 0.1],
            [0.05, 0.05, 0.05],
        ] {
            let back = hlg_to_sdr(sdr_to_hlg(c));
            for (a, b) in back.iter().zip(c) {
                // The knee compresses near white; mid tones stay close.
                assert!(close(*a, b, 0.08), "{c:?} → {back:?}");
            }
        }
    }

    #[test]
    fn pq_sources_convert_for_sdr_and_hlg() {
        // 203 cd/m² PQ white → SDR near white, HLG near 0.75.
        let v = 0.58;
        let sdr = pq_to_sdr([v; 3]);
        assert!(sdr.iter().all(|c| *c > 0.9), "{sdr:?}");
        let hlg = pq_to_hlg([v; 3]);
        assert!(hlg.iter().all(|c| close(*c, 0.75, 0.03)), "{hlg:?}");
    }

    #[test]
    fn frame_conversions_use_the_same_curves_as_the_pixel_functions() {
        let px: Vec<u16> = vec![49_151, 49_151, 49_151, 0, 0, 0, 65_535, 32_768, 0];
        let sdr = hdr16_to_sdr8(&px, SourceTransfer::Hlg);
        assert_eq!(sdr.len(), 12);
        let exact = hlg_to_sdr([49_151.0 / 65_535.0; 3]).map(to_u8);
        assert!(
            sdr[..3].iter().zip(exact).all(|(a, b)| a.abs_diff(b) <= 1),
            "{sdr:?} vs {exact:?}"
        );
        assert_eq!(&sdr[4..8], &[0, 0, 0, 255]);
        let hlg = hdr16_to_hlg16(&px, SourceTransfer::Hlg);
        assert_eq!(
            &hlg[..4],
            &[49_151, 49_151, 49_151, u16::MAX],
            "HLG passes through"
        );
        let from_pq = hdr16_to_hlg16(&[38_010, 38_010, 38_010], SourceTransfer::Pq);
        assert!(from_pq[0].abs_diff(49_151) < 2_000, "{from_pq:?}");
    }
}
