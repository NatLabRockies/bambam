//! Serde helper functions for raw GTFS CSV wire fields (dates, optional times, booleans, floats, hex colors).

use crate::primitives::{GtfsTime, PrimitiveError};
use chrono::NaiveDate;
use chrono_tz::Tz;
use rgb::RGB8;
use serde::de::{self, Deserialize, Deserializer};
use serde::ser::Serializer;

pub(crate) fn deserialize_timezone<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Tz, D::Error> {
    let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(deserializer)?;
    let trimmed = s.trim();
    trimmed
        .parse::<Tz>()
        .map_err(|_| de::Error::custom(PrimitiveError::InvalidTimezone(trimmed.to_owned())))
}

pub(crate) fn serialize_timezone<S: Serializer>(tz: &Tz, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(tz.name())
}

pub(crate) fn deserialize_option_timezone<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Tz>, D::Error> {
    let opt: Option<std::borrow::Cow<'de, str>> = Deserialize::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => {
            let trimmed = s.trim();
            trimmed
                .parse::<Tz>()
                .map(Some)
                .map_err(|_| de::Error::custom(PrimitiveError::InvalidTimezone(trimmed.to_owned())))
        }
    }
}

pub(crate) fn serialize_option_timezone<S: Serializer>(
    tz: &Option<Tz>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match tz {
        None => serializer.serialize_none(),
        Some(t) => serializer.serialize_some(t.name()),
    }
}

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
