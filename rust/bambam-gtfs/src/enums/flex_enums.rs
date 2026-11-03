//! GTFS-Flex booking enumerations (`BookingType`).

use crate::enums::serialize_i16_as_str;
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

/// Describes the type of booking for on-demand services. See <https://gtfs.org/community/extensions/flex/>
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum BookingType {
    /// Real-time booking
    #[default]
    RealTime,
    /// Same-day booking with advance notice
    SameDayWithAdvanceNotice,
    /// Prior day(s) booking
    PriorDays,
    /// An unknown value
    Unknown(i16),
}

impl<'de> Deserialize<'de> for BookingType {
    fn deserialize<D>(deserializer: D) -> Result<BookingType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => BookingType::RealTime,
            "1" => BookingType::SameDayWithAdvanceNotice,
            "2" => BookingType::PriorDays,
            s => BookingType::Unknown(s.parse().map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid value for BookingType, must be an integer: {s}"
                ))
            })?),
        })
    }
}

impl Serialize for BookingType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                BookingType::RealTime => 0,
                BookingType::SameDayWithAdvanceNotice => 1,
                BookingType::PriorDays => 2,
                BookingType::Unknown(i) => *i,
            },
        )
    }
}
