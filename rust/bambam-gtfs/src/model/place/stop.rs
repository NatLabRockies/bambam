//! Physical `Stop`, `StopTransfer`, and `Pathway` entities from `stops.txt`, `transfers.txt`, and `pathways.txt`.

use crate::enums::{Availability, LocationType, PathwayDirectionType, PathwayMode, TransferType};
use crate::ids::*;
use crate::primitives::{
    deserialize_option_timezone, serialize_option_timezone, BoundingBox, Coord, RkyvOptionTz,
};
use crate::raw::{RawPathway, RawStop, RawTransfer};
use crate::traits::{Identifiable, Spatial};
use chrono_tz::Tz;

/// A validated stop-to-stop transfer rule (`transfers.txt`).
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StopTransfer {
    /// Destination stop ID.
    pub to_stop_id: StopId,
    /// Type of transfer connection.
    pub transfer_type: TransferType,
    /// Minimum transfer time in seconds, if specified.
    pub min_transfer_time: Option<u32>,
}

impl From<RawTransfer> for StopTransfer {
    fn from(raw: RawTransfer) -> Self {
        Self {
            to_stop_id: raw.to_stop_id,
            transfer_type: raw.transfer_type,
            min_transfer_time: raw.min_transfer_time,
        }
    }
}

/// A validated station pathway edge (`pathways.txt`).
#[derive(
    Clone,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Pathway {
    /// Unique pathway identifier.
    pub id: String,
    /// Destination stop/node ID.
    pub to_stop_id: StopId,
    /// Pathway mode (walkway, stairs, elevator, etc.).
    pub mode: PathwayMode,
    /// Whether the pathway can be traversed in both directions.
    pub is_bidirectional: PathwayDirectionType,
    /// Horizontal length in meters.
    pub length: Option<f32>,
    /// Average walk traversal time in seconds.
    pub traversal_time: Option<u32>,
}

impl From<RawPathway> for Pathway {
    fn from(raw: RawPathway) -> Self {
        Self {
            id: raw.id,
            to_stop_id: raw.to_stop_id,
            mode: raw.mode,
            is_bidirectional: raw.is_bidirectional,
            length: raw.length,
            traversal_time: raw.traversal_time,
        }
    }
}

/// A physical stop, station, entrance, or node from `stops.txt`.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Stop {
    /// Unique identifier of the stop.
    pub id: StopId,
    /// Optional short rider-facing code.
    pub code: Option<String>,
    /// Optional stop name.
    pub name: Option<String>,
    /// Optional stop description.
    pub description: Option<String>,
    /// Kind of location (stop point, station, entrance, etc.).
    pub location_type: LocationType,
    /// Optional parent station identifier.
    pub parent_station: Option<StopId>,
    /// Optional fare zone identifier.
    pub zone_id: Option<ZoneId>,
    /// Optional URL about the stop.
    pub url: Option<String>,
    /// Geographic coordinate (`lon`, `lat`), either defined directly on this stop
    /// (`stop_lon` and `stop_lat`) or inherited from its `parent_station` during validation.
    pub coord: Option<Coord>,
    /// Optional IANA [`chrono_tz::Tz`] timezone override of the stop (`stop_timezone`).
    #[serde(
        deserialize_with = "deserialize_option_timezone",
        serialize_with = "serialize_option_timezone",
        default
    )]
    #[rkyv(with = RkyvOptionTz)]
    pub timezone: Option<Tz>,
    /// Wheelchair boarding availability.
    pub wheelchair_boarding: Availability,
    /// Optional station level identifier.
    pub level_id: Option<LevelId>,
    /// Optional platform code.
    pub platform_code: Option<String>,
    /// Outgoing transfers from this stop.
    pub transfers: Vec<StopTransfer>,
    /// Outgoing pathways from this stop.
    pub pathways: Vec<Pathway>,
}

impl From<RawStop> for Stop {
    fn from(raw: RawStop) -> Self {
        let coord = match (raw.longitude, raw.latitude) {
            (Some(lon), Some(lat)) => Some(Coord::new(lon, lat)),
            _ => None,
        };
        Self {
            id: raw.id,
            code: raw.code,
            name: raw.name,
            description: raw.description,
            location_type: raw.location_type,
            parent_station: raw.parent_station.filter(|id| !id.is_empty()),
            zone_id: raw.zone_id.filter(|id| !id.is_empty()),
            url: raw.url,
            coord,
            timezone: raw.timezone,
            wheelchair_boarding: raw.wheelchair_boarding,
            level_id: raw.level_id.filter(|id| !id.is_empty()),
            platform_code: raw.platform_code,
            transfers: Vec::new(),
            pathways: Vec::new(),
        }
    }
}

impl Identifiable for Stop {
    type Id = StopId;

    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl Spatial for Stop {
    fn representative_coord(&self) -> Option<Coord> {
        self.coord
    }

    fn bounding_box(&self) -> Option<BoundingBox> {
        self.coord.map(BoundingBox::from_coord)
    }
}
