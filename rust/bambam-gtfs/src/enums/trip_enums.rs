//! Trip, stop-time, calendar, and frequency enumerations (`PickupDropOffType`, `ContinuousPickupDropOff`, `TimepointType`, `Exception`, `DirectionType`, `BikesAllowedType`, `CarsAllowedType`, `ExactTimes`).

use crate::enums::serialize_i16_as_str;
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

/// Describes if and how a traveller can board or alight the vehicle. See <https://gtfs.org/reference/static/#stop_timestxt> `pickup_type` and `dropoff_type`
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
pub enum PickupDropOffType {
    /// Regularly scheduled pickup or drop off (default when empty).
    #[default]
    Regular,
    /// No pickup or drop off available.
    NotAvailable,
    /// Must phone agency to arrange pickup or drop off.
    ArrangeByPhone,
    /// Must coordinate with driver to arrange pickup or drop off.
    CoordinateWithDriver,
    /// An unknown value not in the specification
    Unknown(i16),
}

impl<'de> Deserialize<'de> for PickupDropOffType {
    fn deserialize<D>(deserializer: D) -> Result<PickupDropOffType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => PickupDropOffType::Regular,
            "1" => PickupDropOffType::NotAvailable,
            "2" => PickupDropOffType::ArrangeByPhone,
            "3" => PickupDropOffType::CoordinateWithDriver,
            s => PickupDropOffType::Unknown(s.parse().map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid value for PickupDropOffType, must be an integer: {s}"
                ))
            })?),
        })
    }
}

impl Serialize for PickupDropOffType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                PickupDropOffType::Regular => 0,
                PickupDropOffType::NotAvailable => 1,
                PickupDropOffType::ArrangeByPhone => 2,
                PickupDropOffType::CoordinateWithDriver => 3,
                PickupDropOffType::Unknown(i) => *i,
            },
        )
    }
}

/// Indicates whether a rider can board the transit vehicle anywhere along the vehicle’s travel path
///
/// Those values are only defined on <https://developers.google.com/transit/gtfs/reference#routestxt,> not on <https://gtfs.org/reference/static/#routestxt>
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
pub enum ContinuousPickupDropOff {
    /// Continuous stopping pickup or drop off.
    Continuous,
    /// No continuous stopping pickup or drop off (default when empty).
    #[default]
    NotAvailable,
    /// Must phone agency to arrange continuous stopping pickup or drop off.
    ArrangeByPhone,
    /// Must coordinate with driver to arrange continuous stopping pickup or drop off.
    CoordinateWithDriver,
    /// An unknown value not in the specification
    Unknown(i16),
}

impl Serialize for ContinuousPickupDropOff {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                ContinuousPickupDropOff::Continuous => 0,
                ContinuousPickupDropOff::NotAvailable => 1,
                ContinuousPickupDropOff::ArrangeByPhone => 2,
                ContinuousPickupDropOff::CoordinateWithDriver => 3,
                ContinuousPickupDropOff::Unknown(i) => *i,
            },
        )
    }
}

impl<'de> Deserialize<'de> for ContinuousPickupDropOff {
    fn deserialize<D>(deserializer: D) -> Result<ContinuousPickupDropOff, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "0" => ContinuousPickupDropOff::Continuous,
            "" | "1" => ContinuousPickupDropOff::NotAvailable,
            "2" => ContinuousPickupDropOff::ArrangeByPhone,
            "3" => ContinuousPickupDropOff::CoordinateWithDriver,
            s => ContinuousPickupDropOff::Unknown(s.parse().map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid value for ContinuousPickupDropOff, must be an integer: {s}"
                ))
            })?),
        })
    }
}

/// Describes if the stop time is exact or not. See <https://gtfs.org/reference/static/#stop_timestxt> `timepoint`
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
pub enum TimepointType {
    /// Times are considered approximate
    Approximate = 0,
    /// Times are considered exact
    #[default]
    Exact = 1,
}

impl Serialize for TimepointType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                TimepointType::Approximate => 0,
                TimepointType::Exact => 1,
            },
        )
    }
}

impl<'de> Deserialize<'de> for TimepointType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        match s.as_ref() {
            "" | "1" => Ok(Self::Exact),
            "0" => Ok(Self::Approximate),
            v => Err(serde::de::Error::custom(format!(
                "invalid value for timepoint: {v}"
            ))),
        }
    }
}

/// Defines if a CalendarDate is added or deleted from a Calendar
#[derive(
    Serialize,
    Deserialize,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Clone,
    Copy,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum Exception {
    /// There will be a service on that day
    #[serde(rename = "1")]
    Added,
    /// There won’t be a service on that day
    #[serde(rename = "2")]
    Deleted,
}

/// Defines the direction of a Trip, only for display, not for routing. See <https://gtfs.org/reference/static/#tripstxt> `direction_id`
#[derive(
    Debug,
    Deserialize,
    Serialize,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum DirectionType {
    /// Travel in one direction (e.g. outbound travel).
    #[serde(rename = "0")]
    Outbound,
    /// Travel in the opposite direction (e.g. inbound travel).
    #[serde(rename = "1")]
    Inbound,
}

/// Is the Trip accessible with a bike. See <https://gtfs.org/reference/static/#tripstxt> `bikes_allowed`
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
pub enum BikesAllowedType {
    /// No bike information for the trip
    #[default]
    NoBikeInfo,
    /// Vehicle being used on this particular trip can accommodate at least one bicycle
    AtLeastOneBike,
    /// No bicycles are allowed on this trip
    NoBikesAllowed,
    /// An unknown value not in the specification
    Unknown(i16),
}

impl<'de> Deserialize<'de> for BikesAllowedType {
    fn deserialize<D>(deserializer: D) -> Result<BikesAllowedType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => BikesAllowedType::NoBikeInfo,
            "1" => BikesAllowedType::AtLeastOneBike,
            "2" => BikesAllowedType::NoBikesAllowed,
            s => BikesAllowedType::Unknown(s.parse().map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid value for BikeAllowedType, must be an integer: {s}"
                ))
            })?),
        })
    }
}

impl Serialize for BikesAllowedType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                BikesAllowedType::NoBikeInfo => 0,
                BikesAllowedType::AtLeastOneBike => 1,
                BikesAllowedType::NoBikesAllowed => 2,
                BikesAllowedType::Unknown(i) => *i,
            },
        )
    }
}

/// Is the Trip accessible with a car. See <https://gtfs.org/reference/static/#tripstxt> `cars_allowed`
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
pub enum CarsAllowedType {
    /// No car transport information for the trip
    #[default]
    NoCarInfo,
    /// Vehicle being used on this particular trip can accommodate at least one car
    AtLeastOneCar,
    /// No cars are allowed on this trip
    NoCarsAllowed,
    /// An unknown value not in the specification
    Unknown(i16),
}

impl<'de> Deserialize<'de> for CarsAllowedType {
    fn deserialize<D>(deserializer: D) -> Result<CarsAllowedType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => CarsAllowedType::NoCarInfo,
            "1" => CarsAllowedType::AtLeastOneCar,
            "2" => CarsAllowedType::NoCarsAllowed,
            s => CarsAllowedType::Unknown(s.parse().map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid value for CarsAllowedType, must be an integer: {s}"
                ))
            })?),
        })
    }
}

impl Serialize for CarsAllowedType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                CarsAllowedType::NoCarInfo => 0,
                CarsAllowedType::AtLeastOneCar => 1,
                CarsAllowedType::NoCarsAllowed => 2,
                CarsAllowedType::Unknown(i) => *i,
            },
        )
    }
}

/// Defines if the Frequency is exact (the vehicle runs exactly every n minutes) or not
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum ExactTimes {
    /// Frequency-based trips
    FrequencyBased = 0,
    /// Schedule-based trips with the exact same headway throughout the day.
    ScheduleBased = 1,
}

impl Serialize for ExactTimes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                ExactTimes::FrequencyBased => 0,
                ExactTimes::ScheduleBased => 1,
            },
        )
    }
}

impl<'de> Deserialize<'de> for ExactTimes {
    fn deserialize<D>(deserializer: D) -> Result<ExactTimes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => ExactTimes::FrequencyBased,
            "1" => ExactTimes::ScheduleBased,
            &_ => {
                return Err(serde::de::Error::custom(format!(
                    "Invalid value `{s}`, expected 0 or 1"
                )));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Deserialize)]
    struct TripEnumRow {
        pickup: PickupDropOffType,
        continuous: ContinuousPickupDropOff,
        timepoint: TimepointType,
        bikes: BikesAllowedType,
        cars: CarsAllowedType,
        exact_times: ExactTimes,
    }

    #[test]
    fn test_trip_enums_defaults_and_unknown_fallbacks() {
        let csv_data =
            "pickup,continuous,timepoint,bikes,cars,exact_times\n,,,,,\n1,0,0,1,2,1\n7,8,1,9,10,0\n";
        let mut rdr = csv::Reader::from_reader(csv_data.as_bytes());
        let rows: Vec<TripEnumRow> = rdr.deserialize().map(Result::unwrap).collect();

        assert_eq!(
            rows[0],
            TripEnumRow {
                pickup: PickupDropOffType::Regular,
                continuous: ContinuousPickupDropOff::NotAvailable,
                timepoint: TimepointType::Exact,
                bikes: BikesAllowedType::NoBikeInfo,
                cars: CarsAllowedType::NoCarInfo,
                exact_times: ExactTimes::FrequencyBased,
            }
        );
        assert_eq!(
            rows[1],
            TripEnumRow {
                pickup: PickupDropOffType::NotAvailable,
                continuous: ContinuousPickupDropOff::Continuous,
                timepoint: TimepointType::Approximate,
                bikes: BikesAllowedType::AtLeastOneBike,
                cars: CarsAllowedType::NoCarsAllowed,
                exact_times: ExactTimes::ScheduleBased,
            }
        );
        assert_eq!(
            rows[2],
            TripEnumRow {
                pickup: PickupDropOffType::Unknown(7),
                continuous: ContinuousPickupDropOff::Unknown(8),
                timepoint: TimepointType::Exact,
                bikes: BikesAllowedType::Unknown(9),
                cars: CarsAllowedType::Unknown(10),
                exact_times: ExactTimes::FrequencyBased,
            }
        );
    }
}

