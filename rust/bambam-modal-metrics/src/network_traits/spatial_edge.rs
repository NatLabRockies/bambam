use geo::LineString;
use std::hash::{Hash, Hasher};

/// An edge that exposes the geometry and identity required to participate in a
/// spatial index (R-tree) for neighbor-aware modal penalty computations.
///
/// This is kept separate from [`EdgeForModalMetric`] so that non-spatial
/// callers are not required to provide geometry.
pub trait SpatialEdge {
    /// a stable identifier for this edge, used to exclude an edge from its own
    /// neighbor set during spatial queries.
    fn id(&self) -> String;
    /// Borrowed string slice of the edge identifier when available, avoiding a
    /// heap `String` allocation per edge when hashing IDs.
    fn id_str(&self) -> Option<&str> {
        None
    }
    /// 64-bit hash of the edge identifier used for allocation-free self-exclusion
    /// during spatial R-tree neighbor queries.
    fn id_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(s) = self.id_str() {
            s.hash(&mut hasher);
        } else {
            self.id().hash(&mut hasher);
        }
        hasher.finish()
    }
    /// the linestring geometry of this edge, used to compute its bounding box
    /// and centroid for spatial indexing. Returns `None` when the edge has no
    /// linestring geometry, in which case it is omitted from the spatial index.
    fn linestring(&self) -> Option<&LineString<f32>>;
    /// the source vertex of the edge.
    fn src_vertex_id(&self) -> usize;
}
