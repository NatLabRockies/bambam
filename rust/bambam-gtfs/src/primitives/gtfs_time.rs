//! GTFS service-day time of day (`GtfsTime`).

use crate::primitives::PrimitiveError;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta, Timelike};
use chrono_tz::Tz;
use serde::de::{self, Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};
use std::fmt;

/// Time of day measured in seconds since midnight of the service day.
///
/// Note that GTFS times can exceed `24:00:00` (`86_400` seconds) for overnight trips
/// belonging to the previous service day.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct GtfsTime(pub u32);

impl GtfsTime {
    /// Creates a `GtfsTime` from seconds since midnight.
    #[inline]
    pub const fn from_seconds(seconds: u32) -> Self {
        Self(seconds)
    }

    /// Creates a `GtfsTime` from hours, minutes, and seconds.
    #[inline]
    pub const fn from_hms(hours: u32, minutes: u32, seconds: u32) -> Self {
        Self(hours * 3600 + minutes * 60 + seconds)
    }

    /// Returns the total seconds since midnight.
    #[inline]
    pub const fn seconds(self) -> u32 {
        self.0
    }

    /// Returns a new `GtfsTime` shifted forward by `delta_seconds`.
    #[inline]
    pub const fn add_seconds(self, delta_seconds: u32) -> Self {
        Self(self.0.saturating_add(delta_seconds))
    }

    /// Returns a new `GtfsTime` shifted by a signed `delta_seconds` (clamped at 0).
    #[inline]
    pub const fn add_signed_seconds(self, delta_seconds: i64) -> Self {
        let sum = self.0 as i64 + delta_seconds;
        if sum <= 0 {
            Self(0)
        } else if sum > u32::MAX as i64 {
            Self(u32::MAX)
        } else {
            Self(sum as u32)
        }
    }

    /// Linearly interpolates between `start` and `end` by ratio `t` in `[0.0, 1.0]`.
    #[inline]
    pub fn linear_interpolate(start: Self, end: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let delta = (end.0 as f64 - start.0 as f64) * t;
        Self((start.0 as f64 + delta).round().max(0.0) as u32)
    }

    /// Combines this `GtfsTime` (which may be `>= 24:00:00` for overnight trips) with `service_date`
    /// into a [`NaiveDateTime`].
    pub fn to_naive_datetime(self, service_date: NaiveDate) -> Option<NaiveDateTime> {
        service_date
            .and_hms_opt(0, 0, 0)?
            .checked_add_signed(TimeDelta::seconds(self.0 as i64))
    }

    /// Combines this `GtfsTime` (which may be `>= 24:00:00` for overnight trips) with `service_date`
    /// and a transit agency's IANA [`Tz`] into a timezone-aware [`DateTime<Tz>`].
    ///
    /// Resolves ambiguous wall-clock times (such as DST fall-back) to the earliest occurrence,
    /// and handles spring-forward gaps by advancing from local service-day midnight.
    pub fn to_zoned_datetime(self, service_date: NaiveDate, tz: Tz) -> Option<DateTime<Tz>> {
        let naive = self.to_naive_datetime(service_date)?;
        if let Some(dt) = naive.and_local_timezone(tz).earliest() {
            return Some(dt);
        }
        // Fallback for spring-forward DST gap hours: anchor at local midnight of `service_date`
        // and add the elapsed duration.
        let midnight = service_date
            .and_hms_opt(0, 0, 0)?
            .and_local_timezone(tz)
            .earliest()?;
        midnight.checked_add_signed(TimeDelta::seconds(self.0 as i64))
    }

    /// Extracts the local `(NaiveDate, GtfsTime)` from a timezone-aware [`DateTime`].
    pub fn from_datetime<T: chrono::TimeZone>(dt: &DateTime<T>) -> (NaiveDate, Self) {
        (
            dt.date_naive(),
            Self::from_hms(dt.hour(), dt.minute(), dt.second()),
        )
    }

    /// Parses a `HH:MM:SS` string into a `GtfsTime`.
    pub fn parse(s: &str) -> Result<Self, PrimitiveError> {
        let s = s.trim();
        let mk_err = || PrimitiveError::InvalidTime(s.to_owned());
        if s.len() < 7 {
            return Err(mk_err());
        }
        let mut parts = s.split(':');
        let h_str = parts.next().ok_or_else(mk_err)?;
        let m_str = parts.next().ok_or_else(mk_err)?;
        let s_str = parts.next().ok_or_else(mk_err)?;
        if parts.next().is_some() || m_str.len() != 2 || s_str.len() != 2 {
            return Err(mk_err());
        }
        let h: u32 = h_str.parse().map_err(|_| mk_err())?;
        let m: u32 = m_str.parse().map_err(|_| mk_err())?;
        let sec: u32 = s_str.parse().map_err(|_| mk_err())?;
        if m >= 60 || sec >= 60 {
            return Err(mk_err());
        }
        Ok(Self::from_hms(h, m, sec))
    }
}

impl fmt::Display for GtfsTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02}",
            self.0 / 3600,
            (self.0 % 3600) / 60,
            self.0 % 60
        )
    }
}

impl<'de> Deserialize<'de> for GtfsTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Self::parse(&s).map_err(de::Error::custom)
    }
}

impl Serialize for GtfsTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_gtfs_time_parse_valid_and_invalid() {
        assert_eq!(GtfsTime::parse("00:00:00"), Ok(GtfsTime::from_hms(0, 0, 0)));
        assert_eq!(GtfsTime::parse("08:05:09"), Ok(GtfsTime::from_hms(8, 5, 9)));
        assert_eq!(GtfsTime::parse("8:05:09"), Ok(GtfsTime::from_hms(8, 5, 9)));
        assert_eq!(
            GtfsTime::parse("  25:30:00  "),
            Ok(GtfsTime::from_hms(25, 30, 0))
        );
        assert_eq!(
            GtfsTime::parse("48:00:00"),
            Ok(GtfsTime::from_hms(48, 0, 0))
        );

        for invalid in [
            "12:60:00",
            "12:00:60",
            "12:5:00",
            "12:00:5",
            "12:00",
            "12:00:00:00",
            "-01:00:00",
            "",
            "abc",
        ] {
            assert!(
                matches!(
                    GtfsTime::parse(invalid),
                    Err(PrimitiveError::InvalidTime(_))
                ),
                "expected InvalidTime for '{invalid}'"
            );
        }
    }

    #[test]
    fn test_gtfs_time_arithmetic_lerp_and_display() {
        let t = GtfsTime::from_hms(10, 0, 0);
        assert_eq!(t.seconds(), 36_000);
        assert_eq!(t.add_seconds(3661), GtfsTime::from_hms(11, 1, 1));
        assert_eq!(
            GtfsTime::from_seconds(u32::MAX - 5).add_seconds(10),
            GtfsTime::from_seconds(u32::MAX)
        );

        assert_eq!(t.add_signed_seconds(-3600), GtfsTime::from_hms(9, 0, 0));
        assert_eq!(t.add_signed_seconds(-50_000), GtfsTime::from_seconds(0));
        assert_eq!(
            t.add_signed_seconds(u32::MAX as i64),
            GtfsTime::from_seconds(u32::MAX)
        );

        let start = GtfsTime::from_hms(14, 0, 0);
        let end = GtfsTime::from_hms(15, 0, 0);
        assert_eq!(GtfsTime::linear_interpolate(start, end, 0.0), start);
        assert_eq!(
            GtfsTime::linear_interpolate(start, end, 0.5),
            GtfsTime::from_hms(14, 30, 0)
        );
        assert_eq!(GtfsTime::linear_interpolate(start, end, 1.0), end);
        assert_eq!(GtfsTime::linear_interpolate(start, end, -0.5), start);
        assert_eq!(GtfsTime::linear_interpolate(start, end, 1.5), end);

        let overnight = GtfsTime::from_hms(25, 5, 9);
        assert_eq!(overnight.to_string(), "25:05:09");
        assert_eq!(GtfsTime::parse(&overnight.to_string()), Ok(overnight));
    }

    #[test]
    fn test_gtfs_time_timezone_conversions() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        let overnight = GtfsTime::from_hms(25, 30, 0);
        let zoned = overnight
            .to_zoned_datetime(date, Tz::America__Denver)
            .expect("should convert overnight time to America/Denver");
        assert_eq!(zoned.timezone(), Tz::America__Denver);
        assert_eq!(
            zoned.date_naive(),
            NaiveDate::from_ymd_opt(2026, 7, 16).unwrap()
        );
        let (extracted_date, extracted_time) = GtfsTime::from_datetime(&zoned);
        assert_eq!(
            extracted_date,
            NaiveDate::from_ymd_opt(2026, 7, 16).unwrap()
        );
        assert_eq!(extracted_time, GtfsTime::from_hms(1, 30, 0));

        // Spring-forward DST gap in America/Denver on 2026-03-08 at 02:30:00
        let dst_date = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        let gap_time = GtfsTime::from_hms(2, 30, 0);
        let gap_dt = gap_time
            .to_zoned_datetime(dst_date, Tz::America__Denver)
            .expect("should resolve spring-forward gap time via midnight fallback");
        let midnight = Tz::America__Denver
            .with_ymd_and_hms(2026, 3, 8, 0, 0, 0)
            .unwrap();
        assert_eq!((gap_dt - midnight).num_seconds(), 2 * 3600 + 30 * 60);
    }
}
