use serde::{Deserialize, Serialize};

/// Standardized mapping between a RouteE Compass `vertex_id` and its source Connector/Node ID
/// (`vertices-mapping.csv.gz`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VertexMappingRecord {
    pub vertex_id: usize,
    pub source_id: String,
}
