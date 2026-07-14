use crate::model::osm::graph::OsmWayDataSerializable;
use geo::{BoundingRect, Centroid, Distance, Euclidean};
use rstar::{PointDistance, RTreeObject, AABB};

/// `WayRTreeEntry` wraps `OsmWayDataSerializable` and caches the bounding box
/// and centroid of the way's `linestring`. It is used solely for efficient spatial queries
/// in an R-tree data structure.
///
/// It is used in spatial queries for network analysis, such as computing
/// the Walking Comfort Index (WCI) or Level of Traffic Stress (LTS) for a way using
/// information from the way's geometry, attributes, and nearby ways.
///
/// If we were to implement the `RTreeObject` trait directly on `OsmWayDataSerializable`,
/// we would have to compute the bounding box every time the `envelope()` method
/// is called, which is inefficient.
///
/// This allows us to compute the bounding box and centroid once, and reuse them in O(1)
/// for multiple spatial queries.
#[derive(Clone)]
pub struct WayRTreeEntry {
    bbox: AABB<[f32; 2]>,
    pub centroid: geo::Point<f32>,
    pub way: OsmWayDataSerializable,
}

impl WayRTreeEntry {
    pub fn new(way: OsmWayDataSerializable) -> Option<Self> {
        // Grab the bounding rectangle of the linestring. If it doesn't exist, return None.
        let rect = way.linestring.bounding_rect()?;

        // Compute the centroid of the linestring. If it doesn't exist, return None.
        let centroid = way.linestring.centroid()?;

        // Create the bounding box from the linestring's bounding rectangle
        Some(Self {
            bbox: AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]),
            centroid,
            way,
        })
    }
}

impl RTreeObject for WayRTreeEntry {
    type Envelope = AABB<[f32; 2]>; // Envelope should be the same type as the bbox of WayRTreeEntry
    fn envelope(&self) -> Self::Envelope {
        self.bbox // return the cached bounding box
    }
}

impl PointDistance for WayRTreeEntry {
    // NOTE: The PointDistance trait for WayRTreeEntry uses euclidean distance.
    // We may want to consider using haversine distance since we are working with geographic coordinates.
    // However, for small distances (in the case of local navigation), the difference may be negligible.
    fn distance_2(&self, point: &[f32; 2]) -> f32 {
        let query_point = geo::Point::new(point[0], point[1]);
        let distance = Euclidean.distance(self.centroid, query_point);
        distance * distance
    }
}
