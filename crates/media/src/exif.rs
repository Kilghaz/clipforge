//! EXIF orientation and capture time, plus small date helpers shared with
//! the ffprobe backend.

use std::io::BufReader;
use std::path::Path;

use crate::info::Rotation;

/// What we take from EXIF.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExifSummary {
    pub rotation: Rotation,
    pub captured_at_ms: Option<i64>,
}

/// Reads EXIF from JPEG, TIFF, HEIF, PNG and WebP containers. Files without
/// EXIF yield the default summary; read errors are treated the same way
/// because EXIF is optional metadata.
#[must_use]
pub fn read(path: &Path) -> ExifSummary {
    let Ok(file) = std::fs::File::open(path) else {
        return ExifSummary::default();
    };
    let mut reader = BufReader::new(file);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else {
        return ExifSummary::default();
    };
    summarize(&exif)
}

fn summarize(exif: &exif::Exif) -> ExifSummary {
    let rotation = exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .map_or(Rotation::None, Rotation::from_exif);
    let captured_at_ms = [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ]
    .iter()
    .find_map(|tag| exif.get_field(*tag, exif::In::PRIMARY))
    .and_then(|f| match &f.value {
        exif::Value::Ascii(parts) => parts
            .first()
            .and_then(|bytes| exif::DateTime::from_ascii(bytes).ok()),
        _ => None,
    })
    .and_then(|dt| {
        let offset_min = dt.offset.map_or(0, i64::from);
        civil_to_unix_ms(
            i64::from(dt.year),
            u32::from(dt.month),
            u32::from(dt.day),
            u32::from(dt.hour),
            u32::from(dt.minute),
            u32::from(dt.second),
            dt.nanosecond.unwrap_or(0),
        )
        .map(|ms| ms - offset_min * 60_000)
    });
    ExifSummary {
        rotation,
        captured_at_ms,
    }
}

/// Converts a civil date-time (proleptic Gregorian, treated as UTC) to Unix
/// milliseconds. Returns `None` for impossible dates.
#[must_use]
pub fn civil_to_unix_ms(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
) -> Option<i64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let d = i64::from(day);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second);
    Some(secs * 1_000 + i64::from(nanos / 1_000_000))
}

/// Parses an ISO 8601 timestamp as written by ffprobe
/// (`2024-05-01T10:00:00.000000Z`, `2024-05-01 10:00:00`, with optional
/// `±HH:MM` offset) into Unix milliseconds.
#[must_use]
pub fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, rest) = s.split_at_checked(10)?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let rest = rest.strip_prefix(['T', ' ']).unwrap_or(rest);
    if rest.is_empty() {
        return civil_to_unix_ms(year, month, day, 0, 0, 0, 0);
    }
    // Split off the zone designator.
    let (time, offset_min) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0)
    } else if let Some(idx) = rest.rfind(['+', '-']).filter(|&i| i >= 8) {
        let (t, z) = rest.split_at(idx);
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let z = &z[1..];
        let (zh, zm) = z
            .split_once(':')
            .unwrap_or((z.get(..2).unwrap_or("0"), z.get(2..).unwrap_or("0")));
        let zh: i64 = zh.parse().ok()?;
        let zm: i64 = if zm.is_empty() { 0 } else { zm.parse().ok()? };
        (t, sign * (zh * 60 + zm))
    } else {
        (rest, 0)
    };
    let mut hms = time.splitn(3, ':');
    let hour: u32 = hms.next()?.parse().ok()?;
    let minute: u32 = hms.next()?.parse().ok()?;
    let sec_str = hms.next().unwrap_or("0");
    let (sec, frac) = sec_str.split_once('.').unwrap_or((sec_str, ""));
    let second: u32 = sec.parse().ok()?;
    let nanos: u32 = if frac.is_empty() {
        0
    } else {
        let digits: String = frac.chars().take(9).collect();
        let n: u32 = digits.parse().ok()?;
        n * 10u32.pow(9 - u32::try_from(digits.len()).ok()?)
    };
    civil_to_unix_ms(year, month, day, hour, minute, second, nanos)
        .map(|ms| ms - offset_min * 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn civil_dates_match_known_epochs() {
        assert_eq!(civil_to_unix_ms(1970, 1, 1, 0, 0, 0, 0), Some(0));
        assert_eq!(
            civil_to_unix_ms(2000, 1, 1, 0, 0, 0, 0),
            Some(946_684_800_000)
        );
        assert_eq!(
            civil_to_unix_ms(2024, 2, 29, 12, 30, 15, 500_000_000),
            Some(1_709_209_815_500)
        );
        assert_eq!(civil_to_unix_ms(1969, 12, 31, 23, 59, 59, 0), Some(-1_000));
        assert_eq!(civil_to_unix_ms(2024, 13, 1, 0, 0, 0, 0), None);
        assert_eq!(civil_to_unix_ms(2024, 1, 0, 0, 0, 0, 0), None);
    }

    #[test]
    fn iso8601_variants() {
        let expected = 1_714_557_600_000; // 2024-05-01T10:00:00Z
        assert_eq!(
            parse_iso8601_ms("2024-05-01T10:00:00.000000Z"),
            Some(expected)
        );
        assert_eq!(parse_iso8601_ms("2024-05-01 10:00:00"), Some(expected));
        assert_eq!(
            parse_iso8601_ms("2024-05-01T12:00:00+02:00"),
            Some(expected)
        );
        assert_eq!(
            parse_iso8601_ms("2024-05-01T08:30:00-01:30"),
            Some(expected)
        );
        assert_eq!(
            parse_iso8601_ms("2024-05-01T10:00:00.5Z"),
            Some(expected + 500)
        );
        assert_eq!(
            parse_iso8601_ms("2024-05-01"),
            Some(expected - 10 * 3_600_000)
        );
        assert_eq!(parse_iso8601_ms("garbage"), None);
        assert_eq!(parse_iso8601_ms(""), None);
    }

    #[test]
    fn files_without_exif_give_defaults() {
        assert_eq!(read(&fixture("photo_portrait.png")), ExifSummary::default());
        assert_eq!(
            read(&fixture("broken_truncated.jpg")),
            ExifSummary::default()
        );
        assert_eq!(
            read(Path::new("/definitely/missing.jpg")),
            ExifSummary::default()
        );
    }
}
