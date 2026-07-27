use super::cycleway_tag::CyclewayTag;
use geo::LineString;

/// A vertex (graph node) that carries the attributes required to compute
/// modal penalty scores such as the Walking Comfort Index (WCI).
///
/// Implement this trait for a map provider's vertex/node type
/// (e.g. OpenStreetMap node, Overture Maps connector).
pub trait VertexForModalPenalties {
    /// returns true if the vertex has a traffic signal.
    fn has_traffic_signals(&self) -> bool;
    /// returns true if the vertex has a stop sign.
    fn has_stop_sign(&self) -> bool;
}

/// An edge (graph link) that carries the attributes required to compute
/// modal penalty scores such as the Walking Comfort Index (WCI).
///
/// Implement this trait for a map provider's edge/way type
/// (e.g. OpenStreetMap way, Overture Maps segment).
///
/// The predicates on this trait are "self-contained": they only inspect the
/// edge's own attributes. Neighbor-aware logic (such as inferring walk
/// eligibility or scores from nearby edges) is handled by the WCI compute
/// layer using [`SpatialEdge`] and a spatial index.
pub trait EdgeForModalPenalties {
    /// the posted traffic speed limit for this edge, in miles per hour, if known.
    fn get_traffic_speed_limit(&self) -> Option<i32>;
    /// the cycleway classification for this edge, if the edge carries one.
    /// Returns `None` when the edge has no cycleway attribute, in which case
    /// the compute layer may infer a score from neighboring edges.
    fn get_cycleway_tag(&self) -> Option<CyclewayTag>;
    /// returns true if the edge itself is walk-eligible based on its own
    /// attributes (sidewalk, footway, or a walkable highway type).
    fn is_walkable(&self) -> bool;
    /// returns true if the edge is a sidewalk.
    fn is_sidewalk(&self) -> bool;
    /// returns true if the edge is a footway.
    fn is_footway(&self) -> bool;
    /// returns true if the edge is a low-traffic / low-speed walkable roadway.
    fn is_walkable_highway(&self) -> bool;
}

/// An edge that exposes the geometry and identity required to participate in a
/// spatial index (R-tree) for neighbor-aware modal penalty computations.
///
/// This is kept separate from [`EdgeForModalPenalties`] so that non-spatial
/// callers are not required to provide geometry.
pub trait SpatialEdge {
    /// a stable identifier for this edge, used to exclude an edge from its own
    /// neighbor set during spatial queries.
    fn id(&self) -> String;
    /// the linestring geometry of this edge, used to compute its bounding box
    /// and centroid for spatial indexing. Returns `None` when the edge has no
    /// linestring geometry, in which case it is omitted from the spatial index.
    fn linestring(&self) -> Option<&LineString<f32>>;
}
