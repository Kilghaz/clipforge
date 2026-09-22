//! Integer time in flicks.
//!
//! One flick is 1/705 600 000 s. Every common frame rate (24, 25, 30, 48,
//! 50, 60, 90, 100, 120 fps and the 1001-based NTSC variants) and every
//! common audio sample rate (44.1, 48, 96 kHz) divides it exactly, so
//! timeline arithmetic never accumulates rounding error.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// Number of flicks in one second.
pub const FLICKS_PER_SECOND: i64 = 705_600_000;

/// A duration or timeline position in flicks.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Ticks(i64);

impl Ticks {
    pub const ZERO: Ticks = Ticks(0);
    pub const SECOND: Ticks = Ticks(FLICKS_PER_SECOND);
    pub const MAX: Ticks = Ticks(i64::MAX);

    #[must_use]
    pub const fn from_flicks(flicks: i64) -> Self {
        Ticks(flicks)
    }

    #[must_use]
    pub const fn flicks(self) -> i64 {
        self.0
    }

    /// Whole seconds, exact.
    #[must_use]
    pub const fn from_seconds(seconds: i64) -> Self {
        Ticks(seconds * FLICKS_PER_SECOND)
    }

    /// Milliseconds, exact (one millisecond is 705 600 flicks).
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Ticks(millis * (FLICKS_PER_SECOND / 1000))
    }

    /// Converts a floating point second value, rounding to the nearest flick.
    /// Only meant for the boundary to formats that store floats.
    #[must_use]
    pub fn from_seconds_f64(seconds: f64) -> Self {
        // Precision loss is inherent to the input; rounding is intentional.
        #[allow(clippy::cast_possible_truncation)]
        Ticks((seconds * FLICKS_PER_SECOND as f64).round() as i64)
    }

    /// Seconds as a float, for display and for formats that store floats.
    #[must_use]
    pub fn as_seconds_f64(self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        {
            self.0 as f64 / FLICKS_PER_SECOND as f64
        }
    }

    /// Duration of `frames` frames at `rate`, exact.
    #[must_use]
    pub const fn from_frames(frames: i64, rate: FrameRate) -> Self {
        Ticks(frames * rate.frame_duration().0)
    }

    /// Number of whole frames at `rate` that fit into this duration.
    #[must_use]
    pub const fn to_frames(self, rate: FrameRate) -> i64 {
        self.0.div_euclid(rate.frame_duration().0)
    }

    /// Duration of `samples` audio samples at `sample_rate` Hz, exact for
    /// 44.1 kHz, 48 kHz and 96 kHz.
    #[must_use]
    pub const fn from_samples(samples: i64, sample_rate: u32) -> Self {
        Ticks(samples * (FLICKS_PER_SECOND / sample_rate as i64))
    }

    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    #[must_use]
    pub const fn checked_add(self, rhs: Ticks) -> Option<Ticks> {
        match self.0.checked_add(rhs.0) {
            Some(v) => Some(Ticks(v)),
            None => None,
        }
    }

    #[must_use]
    pub const fn checked_sub(self, rhs: Ticks) -> Option<Ticks> {
        match self.0.checked_sub(rhs.0) {
            Some(v) => Some(Ticks(v)),
            None => None,
        }
    }

    #[must_use]
    pub const fn saturating_add(self, rhs: Ticks) -> Ticks {
        Ticks(self.0.saturating_add(rhs.0))
    }

    #[must_use]
    pub const fn saturating_sub(self, rhs: Ticks) -> Ticks {
        Ticks(self.0.saturating_sub(rhs.0))
    }

    #[must_use]
    pub fn min(self, other: Ticks) -> Ticks {
        Ticks(self.0.min(other.0))
    }

    #[must_use]
    pub fn max(self, other: Ticks) -> Ticks {
        Ticks(self.0.max(other.0))
    }

    #[must_use]
    pub fn clamp(self, lo: Ticks, hi: Ticks) -> Ticks {
        Ticks(self.0.clamp(lo.0, hi.0))
    }
}

impl fmt::Display for Ticks {
    /// Formats as `h:mm:ss.mmm`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let total_ms = self.0.abs() / (FLICKS_PER_SECOND / 1000);
        let ms = total_ms % 1000;
        let s = (total_ms / 1000) % 60;
        let m = (total_ms / 60_000) % 60;
        let h = total_ms / 3_600_000;
        write!(f, "{sign}{h}:{m:02}:{s:02}.{ms:03}")
    }
}

impl Add for Ticks {
    type Output = Ticks;
    fn add(self, rhs: Ticks) -> Ticks {
        Ticks(self.0 + rhs.0)
    }
}
impl AddAssign for Ticks {
    fn add_assign(&mut self, rhs: Ticks) {
        self.0 += rhs.0;
    }
}
impl Sub for Ticks {
    type Output = Ticks;
    fn sub(self, rhs: Ticks) -> Ticks {
        Ticks(self.0 - rhs.0)
    }
}
impl SubAssign for Ticks {
    fn sub_assign(&mut self, rhs: Ticks) {
        self.0 -= rhs.0;
    }
}
impl Mul<i64> for Ticks {
    type Output = Ticks;
    fn mul(self, rhs: i64) -> Ticks {
        Ticks(self.0 * rhs)
    }
}
impl Div<i64> for Ticks {
    type Output = Ticks;
    fn div(self, rhs: i64) -> Ticks {
        Ticks(self.0 / rhs)
    }
}
impl Sum for Ticks {
    fn sum<I: Iterator<Item = Ticks>>(iter: I) -> Ticks {
        iter.fold(Ticks::ZERO, Add::add)
    }
}

/// A frame rate as an exact rational `num / den` frames per second.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate {
    num: u32,
    den: u32,
}

impl FrameRate {
    pub const FPS_24: FrameRate = FrameRate { num: 24, den: 1 };
    pub const FPS_23_976: FrameRate = FrameRate {
        num: 24_000,
        den: 1001,
    };
    pub const FPS_25: FrameRate = FrameRate { num: 25, den: 1 };
    pub const FPS_30: FrameRate = FrameRate { num: 30, den: 1 };
    pub const FPS_29_97: FrameRate = FrameRate {
        num: 30_000,
        den: 1001,
    };
    pub const FPS_50: FrameRate = FrameRate { num: 50, den: 1 };
    pub const FPS_60: FrameRate = FrameRate { num: 60, den: 1 };
    pub const FPS_59_94: FrameRate = FrameRate {
        num: 60_000,
        den: 1001,
    };

    /// Frame rates offered in the project settings, in display order.
    pub const SELECTABLE: [FrameRate; 4] = [Self::FPS_24, Self::FPS_25, Self::FPS_30, Self::FPS_60];

    /// Builds a frame rate. Returns `None` if it does not divide a second in
    /// flicks exactly or if either part is zero.
    #[must_use]
    pub const fn new(num: u32, den: u32) -> Option<FrameRate> {
        if num == 0 || den == 0 {
            return None;
        }
        if (FLICKS_PER_SECOND * den as i64) % num as i64 != 0 {
            return None;
        }
        Some(FrameRate { num, den })
    }

    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.num
    }

    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.den
    }

    /// Exact duration of a single frame.
    #[must_use]
    pub const fn frame_duration(self) -> Ticks {
        Ticks(FLICKS_PER_SECOND * self.den as i64 / self.num as i64)
    }

    /// Frames per second as a float, for display.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

impl Default for FrameRate {
    fn default() -> Self {
        Self::FPS_30
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{} fps", self.num)
        } else {
            write!(f, "{:.2} fps", self.as_f64())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const ALL_RATES: [FrameRate; 8] = [
        FrameRate::FPS_24,
        FrameRate::FPS_23_976,
        FrameRate::FPS_25,
        FrameRate::FPS_30,
        FrameRate::FPS_29_97,
        FrameRate::FPS_50,
        FrameRate::FPS_60,
        FrameRate::FPS_59_94,
    ];

    #[test]
    fn every_supported_rate_divides_a_second_exactly() {
        for rate in ALL_RATES {
            assert!(
                FrameRate::new(rate.num, rate.den).is_some(),
                "{rate} must divide a second in flicks"
            );
            assert_eq!(
                rate.frame_duration() * i64::from(rate.num),
                Ticks::SECOND * i64::from(rate.den)
            );
        }
    }

    #[test]
    fn audio_sample_rates_divide_a_second_exactly() {
        for hz in [44_100_u32, 48_000, 96_000] {
            assert_eq!(FLICKS_PER_SECOND % i64::from(hz), 0);
            assert_eq!(Ticks::from_samples(i64::from(hz), hz), Ticks::SECOND);
        }
    }

    #[test]
    fn rejects_impossible_rates() {
        assert!(FrameRate::new(0, 1).is_none());
        assert!(FrameRate::new(30, 0).is_none());
        assert!(FrameRate::new(13, 1).is_none());
        assert!(FrameRate::new(7, 1).is_some(), "7 divides 705600000");
    }

    #[test]
    fn ntsc_frame_duration_is_exact() {
        assert_eq!(FrameRate::FPS_23_976.frame_duration().flicks(), 29_429_400);
        assert_eq!(FrameRate::FPS_29_97.frame_duration().flicks(), 23_543_520);
    }

    #[test]
    fn display_formats_as_timecode() {
        assert_eq!(Ticks::ZERO.to_string(), "0:00:00.000");
        assert_eq!(Ticks::from_millis(1_500).to_string(), "0:00:01.500");
        assert_eq!(Ticks::from_seconds(3_725).to_string(), "1:02:05.000");
        assert_eq!(
            (Ticks::ZERO - Ticks::from_millis(250)).to_string(),
            "-0:00:00.250"
        );
    }

    #[test]
    fn float_conversion_round_trips_within_one_flick() {
        let t = Ticks::from_seconds_f64(1.234_567);
        let back = Ticks::from_seconds_f64(t.as_seconds_f64());
        assert!((t.flicks() - back.flicks()).abs() <= 1);
    }

    #[test]
    fn serializes_as_plain_integer() {
        let json = serde_json::to_string(&Ticks::from_seconds(2)).unwrap();
        assert_eq!(json, "1411200000");
        let back: Ticks = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Ticks::from_seconds(2));
    }

    proptest! {
        #[test]
        fn frames_round_trip_for_every_rate(frames in 0_i64..1_000_000, idx in 0_usize..ALL_RATES.len()) {
            let rate = ALL_RATES[idx];
            prop_assert_eq!(Ticks::from_frames(frames, rate).to_frames(rate), frames);
        }

        #[test]
        fn to_frames_floors(frames in 0_i64..1_000_000, idx in 0_usize..ALL_RATES.len(), extra in 0_i64..100) {
            let rate = ALL_RATES[idx];
            let d = rate.frame_duration().flicks();
            let extra = extra.min(d - 1);
            let t = Ticks::from_frames(frames, rate) + Ticks::from_flicks(extra);
            prop_assert_eq!(t.to_frames(rate), frames);
        }

        #[test]
        fn sum_equals_repeated_add(parts in proptest::collection::vec(0_i64..1_000_000_000, 0..50)) {
            let expected = parts.iter().fold(Ticks::ZERO, |acc, p| acc + Ticks::from_flicks(*p));
            let actual: Ticks = parts.iter().map(|p| Ticks::from_flicks(*p)).sum();
            prop_assert_eq!(expected, actual);
        }
    }
}
