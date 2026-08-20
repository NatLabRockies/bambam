use geo::{BoundingRect, Centroid, Distance, Euclidean};
use rstar::{PointDistance, RTreeObject, AABB};

use crate::network_traits::spatial_edge::SpatialEdge;

/// The maximum distance (in degrees) within which another edge is considered a
/// spatial "neighbor" for the purpose of neighbor-aware modal penalty scores.
/// Roughly 15 meters at mid latitudes.
pub const MIN_DISTANCE_RTREE_NEIGHBOR: f32 = 1.816e-8;

/// `EdgeRTreeEntry` wraps the network edge and caches the bounding box
/// and centroid of the edge's `linestring`. It is used solely for efficient spatial queries
/// in an R-tree data structure.
///
/// It is used in spatial queries for network analysis, such as computing
/// the Walking Comfort Index (WCI) or Level of Traffic Stress (LTS) for a way using
/// information from the way's geometry, attributes, and nearby ways.
///
/// If we were to implement the `RTreeObject` trait directly on the network edge,
/// we would have to compute the bounding box every time the `envelope()` method
/// is called, which is inefficient.
///
/// This allows us to compute the bounding box and centroid once, and reuse them in O(1)
/// for multiple spatial queries.
#[derive(Clone)]
pub struct EdgeRTreeEntry<E> {
    bbox: AABB<[f32; 2]>,
    pub centroid: geo::Point<f32>,
    pub edge: E,
}

impl<E: SpatialEdge> EdgeRTreeEntry<E> {
    pub fn new(edge: E) -> Option<Self> {
        let linestring = edge.linestring()?;
        // Grab the bounding rectangle of the linestring. If it doesn't exist, return None.
        let rect = linestring.bounding_rect()?;

        // Compute the centroid of the linestring. If it doesn't exist, return None.
        let centroid = linestring.centroid()?;

        // Create the bounding box from the linestring's bounding rectangle
        Some(Self {
            bbox: AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]),
            centroid,
            edge,
        })
    }
}

impl<E: SpatialEdge> RTreeObject for EdgeRTreeEntry<E> {
    type Envelope = AABB<[f32; 2]>; // Envelope should be the same type as the bbox of EdgeRTreeEntry
    fn envelope(&self) -> Self::Envelope {
        self.bbox // return the cached bounding box
    }
}

impl<E: SpatialEdge> PointDistance for EdgeRTreeEntry<E> {
    // NOTE: The PointDistance trait for EdgeRTreeEntry uses euclidean distance.
    // We may want to consider using haversine distance since we are working with geographic coordinates.
    // However, for small distances (in the case of local navigation), the difference may be negligible.
    fn distance_2(&self, point: &[f32; 2]) -> f32 {
        let query_point = geo::Point::new(point[0], point[1]);
        let distance = Euclidean.distance(self.centroid, query_point);
        distance * distance
    }
}

/// Find the neighboring edges within [`MIN_DISTANCE_RTREE_NEIGHBOR`] of the
/// query edge's centroid, excluding the query edge itself (by [`SpatialEdge::id`]).
pub fn find_neighboring_edges<'a, E: SpatialEdge>(
    query: &EdgeRTreeEntry<E>,
    rtree: &'a rstar::RTree<EdgeRTreeEntry<E>>,
) -> Vec<&'a EdgeRTreeEntry<E>> {
    let query_id = query.edge.id();
    rtree
        .locate_within_distance(
            [query.centroid.x(), query.centroid.y()],
            MIN_DISTANCE_RTREE_NEIGHBOR,
        )
        .filter(|entry| entry.edge.id() != query_id)
        .collect()
}
