//! Display formatting. Pure functions, unit-tested, no Slint.

use clipforge_core::Ticks;

/// `1.2 MB` style, decimal units like Finder and Explorer show them.
#[must_use]
pub(crate) fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if n < 1000 {
        return format!("{n} B");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut v = n as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", UNITS[unit])
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// `m:ss` or `h:mm:ss`, rounded to whole seconds.
#[must_use]
pub(crate) fn duration(t: Ticks) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = t.as_seconds_f64().round().max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `2024-05-01 10:00` in UTC. Locale-aware formatting is a later step.
#[must_use]
pub(crate) fn date_time_utc(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem / 60) % 60
    )
}

/// Day, month (1–12) and year of a capture time, for date captions. UTC,
/// like the rest of the capture times.
#[must_use]
pub(crate) fn civil_date(unix_ms: i64) -> (u32, u32, i64) {
    let (y, m, d) = civil_from_days(unix_ms.div_euclid(1000).div_euclid(86_400));
    (d, m, y)
}

/// Howard Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `4032 × 3024`.
#[must_use]
pub(crate) fn dimensions(w: u32, h: u32) -> String {
    format!("{w} × {h}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_decimal_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(999), "999 B");
        assert_eq!(bytes(1_000), "1.0 kB");
        assert_eq!(bytes(1_536_000), "1.5 MB");
        assert_eq!(bytes(250_000_000), "250 MB");
        assert_eq!(bytes(4_700_000_000), "4.7 GB");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(Ticks::ZERO), "0:00");
        assert_eq!(duration(Ticks::from_millis(59_600)), "1:00");
        assert_eq!(duration(Ticks::from_seconds(754)), "12:34");
        assert_eq!(duration(Ticks::from_seconds(3_725)), "1:02:05");
    }

    #[test]
    fn dates() {
        assert_eq!(date_time_utc(0), "1970-01-01 00:00");
        assert_eq!(date_time_utc(1_714_557_600_000), "2024-05-01 10:00");
        assert_eq!(date_time_utc(1_709_209_815_500), "2024-02-29 12:30");
        assert_eq!(date_time_utc(-1_000), "1969-12-31 23:59");
        assert_eq!(dimensions(4032, 3024), "4032 × 3024");
    }

    #[test]
    fn caption_from_date_formats_per_language() {
        // 2026-09-25 12:00 UTC; the layout itself is a translated string.
        assert_eq!(civil_date(1_790_337_600_000), (25, 9, 2026));
        assert_eq!(civil_date(0), (1, 1, 1970));
        assert_eq!(civil_date(-1), (31, 12, 1969));
    }
}
