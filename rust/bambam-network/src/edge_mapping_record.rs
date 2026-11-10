use serde::{Deserialize, Serialize};

use crate::model::SegmentHeading;

/// Standardized mapping between a RouteE Compass `edge_id` and its source Segment/Way ID,
/// linear reference bounds, and travel heading (`<mode>/edges-mapping.csv.gz`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeMappingRecord {
    pub edge_id: usize,
    pub source_id: String,
    pub start_lr: f64,
    pub end_lr: f64,
    pub heading: SegmentHeading,
}
