use crate::common::cycleway_tag::CyclewayTag;
/// An edge (graph link) that carries the attributes required to compute
/// modal metrics such as the Walking Comfort Index (WCI).
///
/// Implement this trait for a map provider's edge/way type
/// (e.g. OpenStreetMap way, Overture Maps segment).
///
/// The predicates on this trait are "self-contained": they only inspect the
/// edge's own attributes.
pub trait EdgeForModalMetric {
    // For both LTS/WCI
    /// the posted traffic speed limit for this edge, in miles per hour, if known.
    fn get_traffic_speed_limit(&self) -> Option<i32>;
    /// the cycleway classification for this edge, if the edge carries one.
    /// Returns `None` when the edge has no cycleway attribute, in which case
    /// the compute layer may infer a score from neighboring edges.
    fn get_cycleway_tag(&self) -> Option<CyclewayTag>;
    // WCI - only
    /// returns true if the edge is walk-eligible based solely on its own attributes.
    fn is_walkable(&self) -> bool;
    /// returns true if the edge is a low-traffic / low-speed walkable roadway.
    fn is_walkable_highway(&self) -> bool;
    /// returns true if the edge is a sidewalk.
    fn is_sidewalk(&self) -> bool;
    /// returns true if the edge is a footway.
    fn is_footway(&self) -> bool;
    // LTS - only
    /// returns true if the edge is unbikeable.
    fn is_unbikeable(&self) -> bool;
    /// returns true if the edge is non-motorized.
    fn is_non_motorized(&self) -> bool;
    /// returns true if the edge is oneway.
    fn is_oneway(&self) -> bool;
}
