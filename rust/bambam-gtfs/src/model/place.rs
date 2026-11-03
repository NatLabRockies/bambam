//! Validated spatial places: [`Stop`], [`FlexLocation`], [`LocationGroup`], and the unified [`StopPlace`] enum.

use crate::enums::{Availability, LocationType, PathwayDirectionType, PathwayMode, TransferType};
use crate::ids::*;
use crate::primitives::{BoundingBox, Coord};
use crate::raw::{RawPathway, RawStop, RawTransfer};
use crate::traits::{Identifiable, Spatial};
use std::sync::Arc;

#[cfg(feature = "gtfs-flex")]
use crate::raw::{RawFlexGeometry, RawFlexLocation};

/// A validated stop-to-stop transfer rule (`transfers.txt`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Geographic coordinate (`lon`, `lat`) if both `stop_lon` and `stop_lat` are present.
    pub coord: Option<Coord>,
    /// Optional timezone of the stop.
    pub timezone: Option<String>,
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

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl Spatial for Stop {
    #[inline]
    fn representative_coord(&self) -> Option<Coord> {
        self.coord
    }

    #[inline]
    fn bounding_box(&self) -> Option<BoundingBox> {
        self.coord.map(BoundingBox::from_coord)
    }
}

/// Validated GeoJSON flexible service zone from `locations.geojson` (GTFS-Flex).
#[cfg(feature = "gtfs-flex")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlexLocation {
    /// Unique identifier of the flexible service zone.
    pub id: LocationId,
    /// Optional human-readable name of the zone.
    pub name: Option<String>,
    /// Optional description of the zone.
    pub description: Option<String>,
    /// Optional fare zone identifier.
    pub zone_id: Option<ZoneId>,
    /// Optional URL about the zone.
    pub url: Option<String>,
    /// Validated Polygon or MultiPolygon geometry.
    pub geometry: RawFlexGeometry,
    /// Precomputed axis-aligned bounding box of the polygon/multipolygon.
    pub bbox: Option<BoundingBox>,
}

#[cfg(feature = "gtfs-flex")]
impl From<RawFlexLocation> for FlexLocation {
    fn from(raw: RawFlexLocation) -> Self {
        let bbox = match &raw.geometry {
            RawFlexGeometry::Polygon(rings) => {
                BoundingBox::from_coords(rings.iter().flat_map(|r| r.iter().copied()))
            }
            RawFlexGeometry::MultiPolygon(polys) => BoundingBox::from_coords(
                polys
                    .iter()
                    .flat_map(|rings| rings.iter().flat_map(|r| r.iter().copied())),
            ),
        };
        Self {
            id: raw.id,
            name: raw.stop_name,
            description: raw.stop_desc,
            zone_id: raw.zone_id.filter(|id| !id.is_empty()),
            url: raw.stop_url,
            geometry: raw.geometry,
            bbox,
        }
    }
}

#[cfg(feature = "gtfs-flex")]
impl Identifiable for FlexLocation {
    type Id = LocationId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

#[cfg(feature = "gtfs-flex")]
impl Spatial for FlexLocation {
    #[inline]
    fn representative_coord(&self) -> Option<Coord> {
        self.bbox.map(|b| b.center())
    }

    #[inline]
    fn bounding_box(&self) -> Option<BoundingBox> {
        self.bbox
    }
}

/// Member of a [`LocationGroup`] or [`StopArea`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PlaceMember {
    /// A fixed stop from `stops.txt`.
    Stop(Arc<Stop>),
    /// A flexible GeoJSON zone from `locations.geojson` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    FlexZone(Arc<FlexLocation>),
}

impl Spatial for PlaceMember {
    fn representative_coord(&self) -> Option<Coord> {
        match self {
            PlaceMember::Stop(s) => s.representative_coord(),
            #[cfg(feature = "gtfs-flex")]
            PlaceMember::FlexZone(z) => z.representative_coord(),
        }
    }

    fn bounding_box(&self) -> Option<BoundingBox> {
        match self {
            PlaceMember::Stop(s) => s.bounding_box(),
            #[cfg(feature = "gtfs-flex")]
            PlaceMember::FlexZone(z) => z.bounding_box(),
        }
    }
}

/// Validated GTFS-Flex location group combining `location_groups.txt` and `location_group_stops.txt`.
#[cfg(feature = "gtfs-flex")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocationGroup {
    /// Unique identifier of the location group.
    pub id: LocationGroupId,
    /// Optional human-readable name of the location group.
    pub name: Option<String>,
    /// Resolved member stops and/or GeoJSON locations.
    pub members: Vec<PlaceMember>,
    /// Precomputed union bounding box of all members.
    pub bbox: Option<BoundingBox>,
}

#[cfg(feature = "gtfs-flex")]
impl LocationGroup {
    /// Constructs a `LocationGroup` and precomputes its union bounding box.
    pub fn new(id: LocationGroupId, name: Option<String>, members: Vec<PlaceMember>) -> Self {
        let bbox = members
            .iter()
            .filter_map(|m| m.bounding_box())
            .reduce(|a, b| a.union(b));
        Self {
            id,
            name,
            members,
            bbox,
        }
    }
}

#[cfg(feature = "gtfs-flex")]
impl Identifiable for LocationGroup {
    type Id = LocationGroupId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

#[cfg(feature = "gtfs-flex")]
impl Spatial for LocationGroup {
    #[inline]
    fn representative_coord(&self) -> Option<Coord> {
        self.bbox.map(|b| b.center())
    }

    #[inline]
    fn bounding_box(&self) -> Option<BoundingBox> {
        self.bbox
    }
}

/// Validated stop area (`stop_areas.txt`) grouping stops and/or flexible locations under an `AreaId`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StopArea {
    /// Area identifier.
    pub id: AreaId,
    /// Resolved members belonging to this area.
    pub members: Vec<PlaceMember>,
}

/// Polymorphic target of a [`StopTime`](crate::model::StopTime).
///
/// Using [`StopPlace`] ensures that `StopTime::place` has a single, consistent type
/// whether `gtfs-flex` is enabled or disabled—preserving Cargo feature additivity!
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StopPlace {
    /// A fixed physical stop from `stops.txt`.
    Stop(Arc<Stop>),
    /// A GeoJSON demand-responsive zone from `locations.geojson` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    FlexZone(Arc<FlexLocation>),
    /// A location group from `location_groups.txt` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    LocationGroup(Arc<LocationGroup>),
}

impl StopPlace {
    /// Returns the underlying [`Stop`] if this place is a fixed stop.
    #[inline]
    pub fn as_stop(&self) -> Option<&Arc<Stop>> {
        match self {
            StopPlace::Stop(s) => Some(s),
            #[cfg(feature = "gtfs-flex")]
            _ => None,
        }
    }

    /// Returns the underlying [`FlexLocation`] if this place is a flexible GeoJSON zone.
    #[cfg(feature = "gtfs-flex")]
    #[inline]
    pub fn as_flex_zone(&self) -> Option<&Arc<FlexLocation>> {
        match self {
            StopPlace::FlexZone(z) => Some(z),
            _ => None,
        }
    }

    /// Returns the underlying [`LocationGroup`] if this place is a location group.
    #[cfg(feature = "gtfs-flex")]
    #[inline]
    pub fn as_location_group(&self) -> Option<&Arc<LocationGroup>> {
        match self {
            StopPlace::LocationGroup(g) => Some(g),
            _ => None,
        }
    }

    /// Returns the string identifier of the referenced stop, flex zone, or location group.
    pub fn raw_id_str(&self) -> &str {
        match self {
            StopPlace::Stop(s) => s.id.as_str(),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::FlexZone(z) => z.id.as_str(),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::LocationGroup(g) => g.id.as_str(),
        }
    }
}

impl Spatial for StopPlace {
    fn representative_coord(&self) -> Option<Coord> {
        match self {
            StopPlace::Stop(s) => s.representative_coord(),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::FlexZone(z) => z.representative_coord(),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::LocationGroup(g) => g.representative_coord(),
        }
    }

    fn bounding_box(&self) -> Option<BoundingBox> {
        match self {
            StopPlace::Stop(s) => s.bounding_box(),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::FlexZone(z) => z.bounding_box(),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::LocationGroup(g) => g.bounding_box(),
        }
    }
}
