use super::penalty_traits::SpatialEdge;
use geo::{BoundingRect, Centroid, Distance, Euclidean};
use rstar::{PointDistance, RTreeObject, AABB};

/// The maximum distance (in degrees) within which another edge is considered a
/// spatial "neighbor" for the purpose of neighbor-aware modal penalty scores.
/// Roughly 15 meters at mid latitudes.
pub const MIN_DISTANCE_RTREE_NEIGHBOR: f32 = 0.0001378;

/// `EdgeRTreeEntry` wraps any edge implementing [`SpatialEdge`] and caches the
/// bounding box and centroid of the edge's linestring. It is used solely for
/// efficient spatial queries in an R-tree.
///
/// This is the map-agnostic replacement for the previously OSM-specific
/// `WayRTreeEntry`. Any map provider (OpenStreetMap, Overture Maps, ...) whose
/// edge type implements [`SpatialEdge`] can be indexed and scored with the same
/// machinery.
///
/// Caching the bounding box and centroid once lets each spatial query run in
/// O(1) rather than recomputing geometry on every `envelope()` call.
#[derive(Clone)]
pub struct EdgeRTreeEntry<E> {
    bbox: AABB<[f32; 2]>,
    pub centroid: geo::Point<f32>,
    pub edge: E,
}

impl<E: SpatialEdge> EdgeRTreeEntry<E> {
    /// Build an entry from an edge, computing and caching its bounding box and
    /// centroid. Returns `None` if the edge geometry has no bounding rectangle
    /// or centroid (e.g. an empty linestring).
    pub fn new(edge: E) -> Option<Self> {
        let linestring = edge.linestring()?;
        let rect = linestring.bounding_rect()?;
        let centroid = linestring.centroid()?;
        Some(Self {
            bbox: AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]),
            centroid,
            edge,
        })
    }
}

impl<E> RTreeObject for EdgeRTreeEntry<E> {
    type Envelope = AABB<[f32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.bbox
    }
}

impl<E> PointDistance for EdgeRTreeEntry<E> {
    // NOTE: uses Euclidean distance on geographic coordinates. For the small
    // distances involved in local navigation the error vs. Haversine is
    // negligible.
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
