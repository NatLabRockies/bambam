//! Polymorphic `StopPlace`, `PlaceMember`, and `StopArea` domain types.

use crate::ids::AreaId;
use crate::model::place::Stop;
#[cfg(feature = "gtfs-flex")]
use crate::model::place::{FlexLocation, LocationGroup};
use crate::primitives::{BoundingBox, Coord};
use crate::traits::Spatial;
use std::sync::Arc;

/// Member of a [`LocationGroup`] or [`StopArea`].
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

/// Validated stop area (`stop_areas.txt`) grouping stops and/or flexible locations under an `AreaId`.
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

    /// Returns `true` if `coord` lies inside this `StopPlace` (exact point-in-polygon for `FlexZone`
    /// or `LocationGroup`, or exact coordinate match for `Stop`).
    pub fn contains_coord(&self, coord: Coord) -> bool {
        match self {
            StopPlace::Stop(s) => s.coord == Some(coord),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::FlexZone(z) => z.contains_coord(coord),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::LocationGroup(g) => g.contains_coord(coord),
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
