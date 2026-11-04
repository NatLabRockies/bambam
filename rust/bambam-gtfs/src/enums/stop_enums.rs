//! Stop, transfer, pathway, and accessibility enumerations (`ObjectType`, `LocationType`, `Availability`, `TransferType`, `PathwayMode`, `PathwayDirectionType`).

use crate::enums::serialize_i16_as_str;
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

/// All the objects type from the GTFS specification that this library reads
#[derive(
    Debug, Serialize, Eq, PartialEq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum ObjectType {
    /// Agency <https://gtfs.org/reference/static/#agencytxt>
    Agency,
    /// Stop <https://gtfs.org/reference/static/#stopstxt>
    Stop,
    /// Route <https://gtfs.org/reference/static/#routestxt>
    Route,
    /// Trip <https://gtfs.org/reference/static/#tripstxt>
    Trip,
    /// Calendar <https://gtfs.org/reference/static/#calendartxt>
    Calendar,
    /// Shape <https://gtfs.org/reference/static/#shapestxt>
    Shape,
    /// Fare <https://gtfs.org/reference/static/#fare_rulestxt>
    Fare,
    /// Pathway <https://gtfs.org/schedule/reference/#pathwaystxt>
    Pathway,
    /// Level <https://gtfs.org/reference/static/#levelstxt>
    Level,
}

/// Describes the kind of Stop. See <https://gtfs.org/reference/static/#stopstxt> `location_type`
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
pub enum LocationType {
    /// Stop (or Platform). A location where passengers board or disembark from a transit vehicle. Is called a platform when defined within a parent_station
    #[default]
    StopPoint,
    /// Station. A physical structure or area that contains one or more platform
    StopArea,
    /// A location where passengers can enter or exit a station from the street. If an entrance/exit belongs to multiple stations, it can be linked by pathways to both, but the data provider must pick one of them as parent
    StationEntrance,
    /// A location within a station, not matching any other `location_type`, which can be used to link together pathways define in pathways.txt.
    GenericNode,
    /// A specific location on a platform, where passengers can board and/or alight vehicles
    BoardingArea,
    /// An unknown value
    Unknown(i16),
}

impl<'de> Deserialize<'de> for LocationType {
    fn deserialize<D>(deserializer: D) -> Result<LocationType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => LocationType::StopPoint,
            "1" => LocationType::StopArea,
            "2" => LocationType::StationEntrance,
            "3" => LocationType::GenericNode,
            "4" => LocationType::BoardingArea,
            s => LocationType::Unknown(s.parse().map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid value for LocationType, must be an integer: {s}"
                ))
            })?),
        })
    }
}

impl Serialize for LocationType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                LocationType::StopPoint => 0,
                LocationType::StopArea => 1,
                LocationType::StationEntrance => 2,
                LocationType::GenericNode => 3,
                LocationType::BoardingArea => 4,
                LocationType::Unknown(i) => *i,
            },
        )
    }
}

/// Generic enum to define if a service (like wheelchair boarding) is available
#[derive(
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Clone,
    Copy,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum Availability {
    /// No information if the service is available
    #[default]
    InformationNotAvailable,
    /// The service is available
    Available,
    /// The service is not available
    NotAvailable,
    /// An unknown value not in the specification
    Unknown(i16),
}

impl<'de> Deserialize<'de> for Availability {
    fn deserialize<D>(deserializer: D) -> Result<Availability, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => Availability::InformationNotAvailable,
            "1" => Availability::Available,
            "2" => Availability::NotAvailable,
            s => Availability::Unknown(s.parse().map_err(|_| {
                serde::de::Error::custom(format!(
                    "invalid value for Availability, must be an integer: {s}"
                ))
            })?),
        })
    }
}

impl Serialize for Availability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                Availability::InformationNotAvailable => 0,
                Availability::Available => 1,
                Availability::NotAvailable => 2,
                Availability::Unknown(i) => *i,
            },
        )
    }
}

/// Defines the type of a StopTransfer
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
pub enum TransferType {
    /// Recommended transfer point between routes
    #[default]
    Recommended,
    /// Departing vehicle waits for arriving one
    Timed,
    /// Transfer requires a minimum amount of time between arrival and departure to ensure a connection.
    MinTime,
    /// Transfer is not possible at this location
    Impossible,
    /// Passengers can stay onboard the same vehicle to transfer from one trip to another
    StayOnBoard,
    /// In-seat transfers aren't allowed between sequential trips.
    /// The passenger must alight from the vehicle and re-board.
    MustAlight,
}

impl Serialize for TransferType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                TransferType::Recommended => 0,
                TransferType::Timed => 1,
                TransferType::MinTime => 2,
                TransferType::Impossible => 3,
                TransferType::StayOnBoard => 4,
                TransferType::MustAlight => 5,
            },
        )
    }
}

impl<'de> Deserialize<'de> for TransferType {
    fn deserialize<D>(deserializer: D) -> Result<TransferType, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => TransferType::Recommended,
            "1" => TransferType::Timed,
            "2" => TransferType::MinTime,
            "3" => TransferType::Impossible,
            "4" => TransferType::StayOnBoard,
            "5" => TransferType::MustAlight,
            s => {
                return Err(serde::de::Error::custom(format!(
                    "Invalid value `{s}`, expected 0, 1, 2, 3, 4, 5"
                )));
            }
        })
    }
}

/// Type of pathway between `from_stop_id` and `to_stop_id`
#[derive(
    Debug,
    Serialize,
    Deserialize,
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
pub enum PathwayMode {
    /// A walkway
    #[serde(rename = "1")]
    #[default]
    Walkway,
    /// Stairs
    #[serde(rename = "2")]
    Stairs,
    /// Moving sidewalk / travelator
    #[serde(rename = "3")]
    MovingSidewalk,
    /// Escalator
    #[serde(rename = "4")]
    Escalator,
    /// Elevator
    #[serde(rename = "5")]
    Elevator,
    /// A pathway that crosses into an area of the station where a
    /// proof of payment is required (usually via a physical payment gate)
    #[serde(rename = "6")]
    FareGate,
    /// Indicates a pathway exiting an area where proof-of-payment is required
    /// into an area where proof-of-payment is no longer required.
    #[serde(rename = "7")]
    ExitGate,
}

/// Indicates in which direction the pathway can be used
#[derive(
    Debug,
    Serialize,
    Deserialize,
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
pub enum PathwayDirectionType {
    /// Unidirectional pathway, it can only be used from `from_stop_id` to `to_stop_id`.
    #[serde(rename = "0")]
    #[default]
    Unidirectional,
    /// Bidirectional pathway, it can be used in the two directions.
    #[serde(rename = "1")]
    Bidirectional,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Deserialize)]
    struct StopEnumRow {
        location_type: LocationType,
        wheelchair: Availability,
        transfer_type: TransferType,
    }

    #[test]
    fn test_stop_enums_defaults_and_unknown_fallbacks() {
        let csv_data = "location_type,wheelchair,transfer_type\n,,\n0,1,2\n99,42,5\n";
        let mut rdr = csv::Reader::from_reader(csv_data.as_bytes());
        let rows: Vec<StopEnumRow> = rdr.deserialize().map(Result::unwrap).collect();

        assert_eq!(
            rows[0],
            StopEnumRow {
                location_type: LocationType::StopPoint,
                wheelchair: Availability::InformationNotAvailable,
                transfer_type: TransferType::Recommended,
            }
        );
        assert_eq!(
            rows[1],
            StopEnumRow {
                location_type: LocationType::StopPoint,
                wheelchair: Availability::Available,
                transfer_type: TransferType::MinTime,
            }
        );
        assert_eq!(
            rows[2],
            StopEnumRow {
                location_type: LocationType::Unknown(99),
                wheelchair: Availability::Unknown(42),
                transfer_type: TransferType::MustAlight,
            }
        );
    }
}

