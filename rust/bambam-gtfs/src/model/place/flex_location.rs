//! Validated GTFS-Flex GeoJSON zone (`FlexLocation`).

use crate::ids::{LocationId, ZoneId};
use crate::primitives::{BoundingBox, Coord};
use crate::raw::{RawFlexGeometry, RawFlexLocation};
use crate::traits::{Identifiable, Spatial};

/// Validated GeoJSON flexible service zone from `locations.geojson` (GTFS-Flex).
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

impl FlexLocation {
    /// Returns `true` if `coord` lies inside or on the boundary of this flexible service zone.
    ///
    /// Performs a fast bounding-box check followed by exact ray-casting over the exterior
    /// and interior hole rings of `self.geometry`.
    #[inline]
    pub fn contains_coord(&self, coord: Coord) -> bool {
        if let Some(bbox) = &self.bbox {
            if !bbox.contains(coord) {
                return false;
            }
        }
        self.geometry.contains_coord(coord)
    }
}

impl Identifiable for FlexLocation {
    type Id = LocationId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

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
