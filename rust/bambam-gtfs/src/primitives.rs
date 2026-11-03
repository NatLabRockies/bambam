//! Shared primitive value types (time, time windows, geographic coordinates, bounding boxes, colors)
//! and serde helpers for `gtfs2`.

use chrono::NaiveDate;
use rgb::RGB8;
use serde::de::{self, Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};
use std::fmt;
use thiserror::Error;

/// Errors that can occur when parsing primitive GTFS values (times, dates, colors, floats).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveError {
    /// A time string is not in valid `HH:MM:SS` format.
    #[error("'{0}' is not a valid GTFS time; expected HH:MM:SS")]
    InvalidTime(String),
    /// A time window has `start > end`.
    #[error("invalid time window: start ({start}) is after end ({end})")]
    InvalidTimeWindow {
        /// Window start time
        start: GtfsTime,
        /// Window end time
        end: GtfsTime,
    },
    /// A date string is not in valid `YYYYMMDD` format.
    #[error("'{0}' is not a valid GTFS date; expected YYYYMMDD")]
    InvalidDate(String),
    /// A color string is not in valid 6-digit hex `RRGGBB` format.
    #[error("'{0}' is not a valid hex color; expected RRGGBB without '#'")]
    InvalidColor(String),
}

/// Time of day measured in seconds since midnight of the service day.
///
/// Note that GTFS times can exceed `24:00:00` (`86_400` seconds) for overnight trips
/// belonging to the previous service day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// An inclusive time interval `[start, end]` on a service day (used for GTFS-Flex pickup/drop-off windows
/// and frequency windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start of the window (inclusive).
    pub start: GtfsTime,
    /// End of the window (inclusive).
    pub end: GtfsTime,
}

impl TimeWindow {
    /// Creates a validated `TimeWindow` where `start <= end`.
    pub fn new(start: GtfsTime, end: GtfsTime) -> Result<Self, PrimitiveError> {
        if start > end {
            return Err(PrimitiveError::InvalidTimeWindow { start, end });
        }
        Ok(Self { start, end })
    }

    /// Returns the duration of the window in seconds.
    #[inline]
    pub const fn duration_seconds(&self) -> u32 {
        self.end.0.saturating_sub(self.start.0)
    }

    /// Returns true if `time` falls within `[self.start, self.end]`.
    #[inline]
    pub const fn contains(&self, time: GtfsTime) -> bool {
        time.0 >= self.start.0 && time.0 <= self.end.0
    }
}

/// A 2D geographic coordinate in WGS84 (`longitude`, `latitude` in degrees).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Coord {
    /// Longitude in decimal degrees (`x`).
    pub lon: f64,
    /// Latitude in decimal degrees (`y`).
    pub lat: f64,
}

impl Coord {
    /// Creates a new `Coord` from longitude and latitude in degrees.
    #[inline]
    pub const fn new(lon: f64, lat: f64) -> Self {
        Self { lon, lat }
    }

    /// Approximate Haversine distance in meters between two WGS84 coordinates.
    pub fn haversine_meters(self, other: Self) -> f64 {
        const EARTH_RADIUS_M: f64 = 6_371_008.8;
        let d_lat = (other.lat - self.lat).to_radians();
        let d_lon = (other.lon - self.lon).to_radians();
        let lat1 = self.lat.to_radians();
        let lat2 = other.lat.to_radians();

        let a = (d_lat * 0.5).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon * 0.5).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).max(0.0).sqrt());
        EARTH_RADIUS_M * c
    }

    /// Linearly interpolates between `a` and `b` by factor `t` in `[0.0, 1.0]`.
    #[inline]
    pub fn lerp(a: Self, b: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            lon: a.lon + (b.lon - a.lon) * t,
            lat: a.lat + (b.lat - a.lat) * t,
        }
    }
}

/// Axis-aligned geographic bounding box in WGS84 coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Minimum longitude (west).
    pub min_lon: f64,
    /// Minimum latitude (south).
    pub min_lat: f64,
    /// Maximum longitude (east).
    pub max_lon: f64,
    /// Maximum latitude (north).
    pub max_lat: f64,
}

impl BoundingBox {
    /// Creates a bounding box containing a single coordinate.
    #[inline]
    pub const fn from_coord(c: Coord) -> Self {
        Self {
            min_lon: c.lon,
            min_lat: c.lat,
            max_lon: c.lon,
            max_lat: c.lat,
        }
    }

    /// Computes the bounding box of an iterator of coordinates, or `None` if empty.
    pub fn from_coords(coords: impl IntoIterator<Item = Coord>) -> Option<Self> {
        let mut iter = coords.into_iter();
        let first = iter.next()?;
        let mut bbox = Self::from_coord(first);
        for c in iter {
            bbox.expand_coord(c);
        }
        Some(bbox)
    }

    /// Expands this bounding box in-place to include `c`.
    #[inline]
    pub fn expand_coord(&mut self, c: Coord) {
        self.min_lon = self.min_lon.min(c.lon);
        self.min_lat = self.min_lat.min(c.lat);
        self.max_lon = self.max_lon.max(c.lon);
        self.max_lat = self.max_lat.max(c.lat);
    }

    /// Returns the union of two bounding boxes.
    #[inline]
    pub fn union(self, other: Self) -> Self {
        Self {
            min_lon: self.min_lon.min(other.min_lon),
            min_lat: self.min_lat.min(other.min_lat),
            max_lon: self.max_lon.max(other.max_lon),
            max_lat: self.max_lat.max(other.max_lat),
        }
    }

    /// Returns the center coordinate of the bounding box.
    #[inline]
    pub fn center(&self) -> Coord {
        Coord {
            lon: (self.min_lon + self.max_lon) * 0.5,
            lat: (self.min_lat + self.max_lat) * 0.5,
        }
    }

    /// Returns true if `c` lies within or on the boundary of this bounding box.
    #[inline]
    pub fn contains(&self, c: Coord) -> bool {
        c.lon >= self.min_lon
            && c.lon <= self.max_lon
            && c.lat >= self.min_lat
            && c.lat <= self.max_lat
    }

    /// Returns true if `self` and `other` intersect (share at least one point).
    #[inline]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min_lon <= other.max_lon
            && self.max_lon >= other.min_lon
            && self.min_lat <= other.max_lat
            && self.max_lat >= other.min_lat
    }
}

// --- Serde helper functions for CSV fields ---

pub(crate) fn deserialize_optional_time<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<GtfsTime>, D::Error> {
    let opt: Option<std::borrow::Cow<'de, str>> = Deserialize::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => GtfsTime::parse(&s).map(Some).map_err(de::Error::custom),
    }
}

pub(crate) fn serialize_optional_time<S: Serializer>(
    time: &Option<GtfsTime>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match time {
        None => serializer.serialize_none(),
        Some(t) => serializer.serialize_some(t),
    }
}

pub(crate) fn deserialize_date<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<NaiveDate, D::Error> {
    let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(deserializer)?;
    NaiveDate::parse_from_str(s.trim(), "%Y%m%d")
        .map_err(|_| de::Error::custom(PrimitiveError::InvalidDate(s.into_owned())))
}

pub(crate) fn serialize_date<S: Serializer>(
    date: &NaiveDate,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&date.format("%Y%m%d").to_string())
}

pub(crate) fn deserialize_option_date<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NaiveDate>, D::Error> {
    let opt: Option<std::borrow::Cow<'de, str>> = Deserialize::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => NaiveDate::parse_from_str(s.trim(), "%Y%m%d")
            .map(Some)
            .map_err(|_| de::Error::custom(PrimitiveError::InvalidDate(s.into_owned()))),
    }
}

pub(crate) fn serialize_option_date<S: Serializer>(
    date: &Option<NaiveDate>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match date {
        None => serializer.serialize_none(),
        Some(d) => serializer.serialize_some(&d.format("%Y%m%d").to_string()),
    }
}

pub(crate) fn deserialize_bool<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(deserializer)?;
    match s.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        other => Err(de::Error::custom(format!(
            "Invalid boolean value `{other}`, expected 0 or 1"
        ))),
    }
}

pub(crate) fn serialize_bool<S: Serializer>(b: &bool, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(if *b { "1" } else { "0" })
}

pub(crate) fn de_with_optional_float<'de, D: Deserializer<'de>>(
    de: D,
) -> Result<Option<f64>, D::Error> {
    let s = Option::<std::borrow::Cow<'de, str>>::deserialize(de)?;
    match s {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => s.trim().parse::<f64>().map(Some).map_err(de::Error::custom),
    }
}

pub(crate) fn serialize_float_as_str<S: Serializer>(
    float: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match float {
        None => serializer.serialize_none(),
        Some(f) => serializer.serialize_some(&f.to_string()),
    }
}

pub(crate) fn parse_color(s: &str) -> Result<RGB8, PrimitiveError> {
    let s = s.trim();
    if s.len() != 6 {
        return Err(PrimitiveError::InvalidColor(s.to_owned()));
    }
    let r =
        u8::from_str_radix(&s[0..2], 16).map_err(|_| PrimitiveError::InvalidColor(s.to_owned()))?;
    let g =
        u8::from_str_radix(&s[2..4], 16).map_err(|_| PrimitiveError::InvalidColor(s.to_owned()))?;
    let b =
        u8::from_str_radix(&s[4..6], 16).map_err(|_| PrimitiveError::InvalidColor(s.to_owned()))?;
    Ok(RGB8::new(r, g, b))
}

pub(crate) fn deserialize_optional_color<'de, D: Deserializer<'de>>(
    de: D,
) -> Result<Option<RGB8>, D::Error> {
    let opt = Option::<std::borrow::Cow<'de, str>>::deserialize(de)?;
    match opt {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => parse_color(&s).map(Some).map_err(de::Error::custom),
    }
}

pub(crate) fn serialize_optional_color<S: Serializer>(
    color: &Option<RGB8>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match color {
        Some(c) => serializer.serialize_some(&format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b)),
        None => serializer.serialize_none(),
    }
}
