/// A vertex (graph node) that carries the attributes required to compute
/// modal metrics such as the Walking Comfort Index (WCI) or Level of Traffic Stress (LTS).
///
/// Implement this trait for a map provider's vertex/node type
/// (e.g. OpenStreetMap node, Overture Maps connector).
pub trait VertexForModalMetric {
    // WCI - only
    /// returns true if the vertex has a traffic signal.
    fn has_traffic_signals(&self) -> bool;
    /// returns true if the vertex has a stop sign.
    fn has_stop_sign(&self) -> bool;
}
