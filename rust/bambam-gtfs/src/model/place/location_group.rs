//! Validated GTFS-Flex `LocationGroup` combining `location_groups.txt` and `location_group_stops.txt`.

use crate::ids::LocationGroupId;
use crate::model::place::PlaceMember;
use crate::primitives::{BoundingBox, Coord};
use crate::traits::{Identifiable, Spatial};

/// Validated GTFS-Flex location group combining `location_groups.txt` and `location_group_stops.txt`.
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

    /// Returns `true` if `coord` lies inside any member `FlexZone` or matches a member `Stop` coordinate.
    pub fn contains_coord(&self, coord: Coord) -> bool {
        if let Some(bbox) = &self.bbox {
            if !bbox.contains(coord) {
                return false;
            }
        }
        self.members.iter().any(|m| match m {
            PlaceMember::Stop(s) => s.coord == Some(coord),
            PlaceMember::FlexZone(z) => z.contains_coord(coord),
        })
    }
}

impl Identifiable for LocationGroup {
    type Id = LocationGroupId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

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
