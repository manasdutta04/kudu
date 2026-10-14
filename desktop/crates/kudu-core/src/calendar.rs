//! Calendar days and ISO-8601 timestamps, without a date-time crate.
//!
//! Usage statistics work in whole local days. Core code doesn't know the local time zone, so
//! a [`Calendar`] carries the UTC offset (fixed, or computed per instant by the platform layer
//! so daylight saving time is handled) and turns instants into [`Day`]s.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: i64 = 86_400;

/// A calendar day in the proleptic Gregorian calendar, stored as days since 1 January 1970.
/// Serialized as `"yyyy-MM-dd"`, the keys of the macOS app's `usage.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Day(i32);

impl Day {
    pub const fn from_days_since_epoch(days: i32) -> Self {
        Self(days)
    }

    pub const fn days_since_epoch(self) -> i32 {
        self.0
    }

    /// The day with this year, month (1–12) and day of month, if it exists.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self(days_from_civil(year, month, day)))
    }

    /// (year, month 1–12, day of month 1–31).
    pub fn ymd(self) -> (i32, u32, u32) {
        civil_from_days(self.0)
    }

    /// 1 = Sunday … 7 = Saturday, like `Calendar.component(.weekday, …)`.
    pub fn weekday(self) -> u32 {
        // 1 January 1970 was a Thursday.
        (self.0 + 4).rem_euclid(7) as u32 + 1
    }

    pub fn add_days(self, days: i32) -> Self {
        Self(self.0 + days)
    }

    /// Moves by whole months, keeping the day of month where it exists and otherwise using
    /// the month's last day (31 March − 1 month = 28 or 29 February).
    pub fn add_months(self, months: i32) -> Self {
        let (year, month, day) = self.ymd();
        let index = year * 12 + (month as i32 - 1) + months;
        let (year, month) = (index.div_euclid(12), index.rem_euclid(12) as u32 + 1);
        Self(days_from_civil(year, month, day.min(days_in_month(year, month))))
    }

    pub fn first_of_month(self) -> Self {
        let (year, month, _) = self.ymd();
        Self(days_from_civil(year, month, 1))
    }

    /// Days from `earlier` to `self`.
    pub fn days_since(self, earlier: Day) -> i32 {
        self.0 - earlier.0
    }
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = self.ymd();
        write!(f, "{year:04}-{month:02}-{day:02}")
    }
}

/// A `Day` that couldn't be read from text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidDay;

impl fmt::Display for InvalidDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a date as yyyy-MM-dd")
    }
}

impl std::error::Error for InvalidDay {}

impl FromStr for Day {
    type Err = InvalidDay;

    fn from_str(text: &str) -> Result<Self, InvalidDay> {
        let mut parts = text.splitn(3, '-');
        let mut next = || parts.next().and_then(|part| part.parse::<i64>().ok());
        let (year, month, day) = (next().ok_or(InvalidDay)?, next().ok_or(InvalidDay)?, next().ok_or(InvalidDay)?);
        let year = i32::try_from(year).map_err(|_| InvalidDay)?;
        let month = u32::try_from(month).map_err(|_| InvalidDay)?;
        let day = u32::try_from(day).map_err(|_| InvalidDay)?;
        Day::from_ymd(year, month, day).ok_or(InvalidDay)
    }
}

impl Serialize for Day {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Day {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = std::borrow::Cow::<str>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Turns instants into local days and times. `Calendar::utc()` is the default; the platform
/// layer builds one from the system time zone with [`Calendar::with_offset`].
#[derive(Clone)]
pub struct Calendar {
    offset: Offset,
}

#[derive(Clone)]
enum Offset {
    Fixed(i32),
    Varying(Arc<dyn Fn(SystemTime) -> i32 + Send + Sync>),
}

impl Calendar {
    pub fn utc() -> Self {
        Self { offset: Offset::Fixed(0) }
    }

    /// A time zone that is always `seconds_east` ahead of UTC (negative for the Americas).
    pub fn fixed_offset(seconds_east: i32) -> Self {
        Self { offset: Offset::Fixed(seconds_east) }
    }

    /// A time zone whose offset from UTC (seconds east) depends on the instant, for daylight
    /// saving time. The function is usually the platform's local-time conversion.
    pub fn with_offset(offset: impl Fn(SystemTime) -> i32 + Send + Sync + 'static) -> Self {
        Self { offset: Offset::Varying(Arc::new(offset)) }
    }

    /// Seconds east of UTC at `instant`.
    pub fn utc_offset(&self, instant: SystemTime) -> i32 {
        match &self.offset {
            Offset::Fixed(seconds) => *seconds,
            Offset::Varying(offset) => offset(instant),
        }
    }

    /// The local day `instant` falls on (`startOfDay(for:)`).
    pub fn day(&self, instant: SystemTime) -> Day {
        self.local(instant).0
    }

    /// The local day and the seconds since its midnight.
    pub fn local(&self, instant: SystemTime) -> (Day, u32) {
        let local = unix_seconds(instant) + i64::from(self.utc_offset(instant));
        let day = local.div_euclid(SECONDS_PER_DAY);
        (Day(day as i32), local.rem_euclid(SECONDS_PER_DAY) as u32)
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::utc()
    }
}

impl fmt::Debug for Calendar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.offset {
            Offset::Fixed(seconds) => write!(f, "Calendar(UTC{seconds:+}s)"),
            Offset::Varying(_) => f.write_str("Calendar(local)"),
        }
    }
}

/// Whole seconds since 1970 (rounded down, also before 1970).
pub fn unix_seconds(instant: SystemTime) -> i64 {
    match instant.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs() as i64,
        Err(error) => {
            let before = error.duration();
            -(before.as_secs() as i64) - i64::from(before.subsec_nanos() > 0)
        }
    }
}

/// The instant `seconds` (and `nanos`) after 1970.
pub fn from_unix_seconds(seconds: i64, nanos: u32) -> SystemTime {
    if seconds >= 0 {
        UNIX_EPOCH + Duration::new(seconds as u64, nanos)
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs()) + Duration::from_nanos(u64::from(nanos))
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Howard Hinnant's days-from-civil algorithms.
fn days_from_civil(year: i32, month: u32, day: u32) -> i32 {
    let year = i64::from(if month <= 2 { year - 1 } else { year });
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted_month = (i64::from(month) + 9) % 12; // March = 0
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    (era * 146_097 + day_of_era - 719_468) as i32
}

fn civil_from_days(days: i32) -> (i32, u32, u32) {
    let days = i64::from(days) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year as i32, month, day)
}

/// ISO-8601 timestamps as the macOS app writes them in `history.jsonl`
/// (`JSONEncoder.DateEncodingStrategy.iso8601`): `2026-10-14T15:00:00Z`, whole seconds, UTC.
///
/// Use on a `SystemTime` field with `#[serde(with = "kudu_core::calendar::iso8601")]`.
pub mod iso8601 {
    use super::*;

    /// `2026-10-14T15:00:00Z` (fractions of a second are dropped, as on macOS).
    pub fn format(instant: SystemTime) -> String {
        let seconds = unix_seconds(instant);
        let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_PER_DAY) as i32);
        let time = seconds.rem_euclid(SECONDS_PER_DAY);
        format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", time / 3600, time / 60 % 60, time % 60)
    }

    /// Reads `yyyy-MM-ddTHH:mm:ss` with an optional fraction and a `Z` or `±HH:mm` / `±HHmm`
    /// / `±HH` offset.
    pub fn parse(text: &str) -> Option<SystemTime> {
        let bytes = text.as_bytes();
        let number = |range: std::ops::Range<usize>| -> Option<i64> {
            let digits = text.get(range)?;
            digits.bytes().all(|b| b.is_ascii_digit()).then(|| digits.parse().ok())?
        };
        if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || !matches!(bytes[10], b'T' | b't' | b' ') {
            return None;
        }
        if bytes[13] != b':' || bytes[16] != b':' {
            return None;
        }
        let year = i32::try_from(number(0..4)?).ok()?;
        let day = Day::from_ymd(year, number(5..7)? as u32, number(8..10)? as u32)?;
        let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }
        let mut rest = &text[19..];
        let mut nanos = 0u32;
        if let Some(fraction) = rest.strip_prefix(['.', ',']) {
            let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            for (index, digit) in fraction[..digits].bytes().take(9).enumerate() {
                nanos += u32::from(digit - b'0') * 10u32.pow(8 - index as u32);
            }
            rest = &fraction[digits..];
        }
        let offset = match rest {
            "Z" | "z" => 0,
            _ => {
                let sign = match rest.as_bytes().first()? {
                    b'+' => 1,
                    b'-' => -1,
                    _ => return None,
                };
                let digits: String = rest[1..].chars().filter(|&c| c != ':').collect();
                if !digits.bytes().all(|b| b.is_ascii_digit()) || !matches!(digits.len(), 2 | 4) {
                    return None;
                }
                let hours: i64 = digits[..2].parse().ok()?;
                let minutes: i64 = if digits.len() == 4 { digits[2..].parse().ok()? } else { 0 };
                sign * (hours * 3600 + minutes * 60)
            }
        };
        let seconds = i64::from(day.days_since_epoch()) * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second - offset;
        Some(from_unix_seconds(seconds, nanos))
    }

    pub fn serialize<S: Serializer>(instant: &SystemTime, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format(*instant))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<SystemTime, D::Error> {
        let text = std::borrow::Cow::<str>::deserialize(deserializer)?;
        parse(&text).ok_or_else(|| serde::de::Error::custom(format!("invalid ISO-8601 date: {text}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_days_and_weekdays() {
        let day = Day::from_ymd(2026, 10, 14).unwrap();
        assert_eq!(day.days_since_epoch(), 20_740);
        assert_eq!(day.ymd(), (2026, 10, 14));
        assert_eq!(day.weekday(), 4); // Wednesday
        assert_eq!(Day::from_days_since_epoch(0).weekday(), 5); // Thursday
        assert_eq!(Day::from_ymd(1969, 12, 31).unwrap().days_since_epoch(), -1);
        assert_eq!(Day::from_ymd(2026, 2, 29), None);
        assert_eq!(day.to_string(), "2026-10-14");
        assert_eq!("2026-10-14".parse::<Day>(), Ok(day));
        assert!("2026-13-01".parse::<Day>().is_err());
    }

    #[test]
    fn adds_months_clamping_the_day() {
        let day = Day::from_ymd(2024, 3, 31).unwrap();
        assert_eq!(day.add_months(-1).ymd(), (2024, 2, 29));
        assert_eq!(day.add_months(-3).ymd(), (2023, 12, 31));
        assert_eq!(day.add_months(10).ymd(), (2025, 1, 31));
        assert_eq!(day.first_of_month().ymd(), (2024, 3, 1));
    }

    #[test]
    fn local_days_follow_the_offset() {
        let instant = from_unix_seconds(1_791_990_000, 0); // 14 October 2026, 15:00 UTC
        assert_eq!(Calendar::utc().local(instant), (Day::from_ymd(2026, 10, 14).unwrap(), 15 * 3600));
        assert_eq!(Calendar::fixed_offset(10 * 3600).day(instant).ymd(), (2026, 10, 15));
        assert_eq!(Calendar::with_offset(|_| -16 * 3600).day(instant).ymd(), (2026, 10, 13));
    }

    #[test]
    fn iso8601_round_trips() {
        let instant = from_unix_seconds(1_791_990_000, 0);
        assert_eq!(iso8601::format(instant), "2026-10-14T15:00:00Z");
        assert_eq!(iso8601::parse("2026-10-14T15:00:00Z"), Some(instant));
        assert_eq!(iso8601::parse("2026-10-14T20:30:00+05:30"), Some(instant));
        assert_eq!(iso8601::parse("2026-10-14T15:00:00.250Z"), Some(from_unix_seconds(1_791_990_000, 250_000_000)));
        assert_eq!(iso8601::format(from_unix_seconds(-1, 0)), "1969-12-31T23:59:59Z");
        assert_eq!(iso8601::parse("2026-10-14 nonsense"), None);
    }
}
