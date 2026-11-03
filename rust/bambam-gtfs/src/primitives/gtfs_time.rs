//! GTFS service-day time of day (`GtfsTime`).

use crate::primitives::PrimitiveError;
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
    pub fn lerp(start: Self, end: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let delta = (end.0 as f64 - start.0 as f64) * t;
        Self((start.0 as f64 + delta).round().max(0.0) as u32)
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
