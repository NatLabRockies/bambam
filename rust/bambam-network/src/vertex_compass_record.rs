use routee_compass_core::model::network::Vertex;
use serde::{Deserialize, Serialize};

/// RouteE Compass vertex coordinate record (`vertices-compass.csv.gz`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexCompassRecord {
    pub vertex_id: usize,
    pub x: f32,
    pub y: f32,
}

impl From<Vertex> for VertexCompassRecord {
    fn from(value: Vertex) -> Self {
        Self {
            vertex_id: value.vertex_id.0,
            x: value.x(),
            y: value.y(),
        }
    }
}
