//! R*-tree spatial index entry (`PlaceSpatialEntry`), `rkyv` / `serde` adapters, and flex zone proximity helpers.

use crate::ids::PlaceIdx;
use crate::model::StopPlace;
use crate::primitives::BoundingBox;
#[cfg(feature = "gtfs-flex")]
use crate::primitives::Coord;
use crate::traits::Spatial;
use rstar::{PointDistance, RTree, RTreeObject, AABB};

/// Spatial index entry associating a [`PlaceIdx`] with its WGS84 bounding box inside an [`rstar::RTree`].
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PlaceSpatialEntry {
    /// Contiguous index of the indexed [`StopPlace`].
    pub place_idx: PlaceIdx,
    /// Bounding box of the place (`[min_lon, min_lat]` to `[max_lon, max_lat]`).
    pub bbox: BoundingBox,
}

impl RTreeObject for PlaceSpatialEntry {
    type Envelope = AABB<[f64; 2]>;

    #[inline]
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.bbox.min_lon, self.bbox.min_lat],
            [self.bbox.max_lon, self.bbox.max_lat],
        )
    }
}

impl PointDistance for PlaceSpatialEntry {
    #[inline]
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        self.envelope().distance_2(point)
    }
}

pub(crate) fn build_place_spatial_index(places: &[StopPlace]) -> RTree<PlaceSpatialEntry> {
    let entries: Vec<PlaceSpatialEntry> = places
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            p.bounding_box().map(|bbox| PlaceSpatialEntry {
                place_idx: PlaceIdx::from_usize(i),
                bbox,
            })
        })
        .collect();
    RTree::bulk_load(entries)
}

pub(crate) fn serialize_spatial_index<S: serde::Serializer>(
    tree: &RTree<PlaceSpatialEntry>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    let mut entries: Vec<PlaceSpatialEntry> = tree.iter().copied().collect();
    entries.sort_by_key(|e| e.place_idx);
    entries.serialize(serializer)
}

pub(crate) fn deserialize_spatial_index<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<RTree<PlaceSpatialEntry>, D::Error> {
    use serde::Deserialize;
    let entries = Vec::<PlaceSpatialEntry>::deserialize(deserializer)?;
    Ok(RTree::bulk_load(entries))
}

/// `rkyv::with` wrapper that serializes `rstar::RTree<PlaceSpatialEntry>` as a sorted `Vec<PlaceSpatialEntry>`
/// and bulk-loads the R-tree on deserialization.
pub struct RkyvSpatialIndex;

impl rkyv::with::ArchiveWith<RTree<PlaceSpatialEntry>> for RkyvSpatialIndex {
    type Archived = rkyv::vec::ArchivedVec<ArchivedPlaceSpatialEntry>;
    type Resolver = rkyv::vec::VecResolver;

    fn resolve_with(
        field: &RTree<PlaceSpatialEntry>,
        resolver: Self::Resolver,
        out: rkyv::Place<Self::Archived>,
    ) {
        let mut entries: Vec<PlaceSpatialEntry> = field.iter().copied().collect();
        entries.sort_by_key(|e| e.place_idx);
        rkyv::vec::ArchivedVec::resolve_from_slice(&entries, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized>
    rkyv::with::SerializeWith<RTree<PlaceSpatialEntry>, S> for RkyvSpatialIndex
{
    fn serialize_with(
        field: &RTree<PlaceSpatialEntry>,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        let mut entries: Vec<PlaceSpatialEntry> = field.iter().copied().collect();
        entries.sort_by_key(|e| e.place_idx);
        rkyv::vec::ArchivedVec::serialize_from_slice(&entries, serializer)
    }
}

impl<D>
    rkyv::with::DeserializeWith<
        rkyv::vec::ArchivedVec<ArchivedPlaceSpatialEntry>,
        RTree<PlaceSpatialEntry>,
        D,
    > for RkyvSpatialIndex
where
    D: rkyv::rancor::Fallible + ?Sized,
    D::Error: rkyv::rancor::Source,
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<ArchivedPlaceSpatialEntry>,
        deserializer: &mut D,
    ) -> Result<RTree<PlaceSpatialEntry>, D::Error> {
        let entries: Vec<PlaceSpatialEntry> = rkyv::Deserialize::deserialize(field, deserializer)?;
        Ok(RTree::bulk_load(entries))
    }
}

#[cfg(feature = "gtfs-flex")]
pub(crate) fn flex_zone_within_radius(
    zone: &crate::model::FlexLocation,
    target: Coord,
    radius_meters: f64,
) -> bool {
    let check_ring = |ring: &[Coord]| -> bool {
        if ring
            .iter()
            .any(|&v| v.haversine_meters(target) <= radius_meters)
        {
            return true;
        }
        let cos_lat = target.lat.to_radians().cos().max(1e-6);
        for seg in ring.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let ax = (a.lon - target.lon) * cos_lat;
            let ay = a.lat - target.lat;
            let bx = (b.lon - target.lon) * cos_lat;
            let by = b.lat - target.lat;
            let dx = bx - ax;
            let dy = by - ay;
            let len_sq = dx * dx + dy * dy;
            let t = if len_sq > 0.0 {
                (-(ax * dx + ay * dy) / len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let closest = Coord::linear_interpolate(a, b, t);
            if closest.haversine_meters(target) <= radius_meters {
                return true;
            }
        }
        false
    };

    match &zone.geometry {
        crate::raw::RawFlexGeometry::Polygon(rings) => rings.iter().any(|r| check_ring(r)),
        crate::raw::RawFlexGeometry::MultiPolygon(polys) => polys
            .iter()
            .any(|rings| rings.iter().any(|r| check_ring(r))),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gtfs-flex")]
    use super::*;
    #[cfg(feature = "gtfs-flex")]
    use crate::ids::LocationId;
    #[cfg(feature = "gtfs-flex")]
    use crate::model::{FlexLocation, ValidatedFeed};
    #[cfg(feature = "gtfs-flex")]
    use crate::raw::{RawFeed, RawFlexGeometry, RawFlexLocation};
    #[cfg(feature = "gtfs-flex")]
    use crate::routing::RaptorTimetable;
    #[cfg(feature = "gtfs-flex")]
    use std::path::PathBuf;

    #[cfg(feature = "gtfs-flex")]
    #[test]
    fn test_places_near_with_rtree_and_concave_polygon() {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/basic");
        let l_ring = vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 0.4),
            Coord::new(0.4, 0.4),
            Coord::new(0.4, 1.0),
            Coord::new(0.0, 1.0),
            Coord::new(0.0, 0.0),
        ];
        let raw_loc = RawFlexLocation {
            id: LocationId::new("l_zone"),
            stop_name: Some("L-Shaped Flex Zone".to_owned()),
            stop_desc: None,
            zone_id: None,
            stop_url: None,
            geometry: RawFlexGeometry::Polygon(vec![l_ring]),
        };
        let flex_loc = FlexLocation::from(raw_loc.clone());

        let inside_bottom = Coord::new(0.8, 0.2);
        let outside_in_bbox_cutout = Coord::new(0.8, 0.8);
        assert!(flex_loc.bbox.unwrap().contains(outside_in_bbox_cutout));
        assert!(!flex_loc.contains_coord(outside_in_bbox_cutout));

        let mut raw = RawFeed::from_path(fixture_dir).unwrap();
        raw.locations = Some(vec![raw_loc]);
        let validated = ValidatedFeed::try_from(raw).unwrap();
        let timetable = RaptorTimetable::try_from(validated).unwrap();

        let l_zone_pidx = timetable
            .lookup_flex_location(
                &crate::ids::FeedId::new("default"),
                &LocationId::new("l_zone"),
            )
            .unwrap();
        assert!(timetable
            .places_near(inside_bottom, 0.0)
            .contains(&l_zone_pidx));
        assert!(
            !timetable
                .places_near(outside_in_bbox_cutout, 0.0)
                .contains(&l_zone_pidx),
            "places_near(radius=0) should not match L-zone when point is in the cutout"
        );
    }
}
