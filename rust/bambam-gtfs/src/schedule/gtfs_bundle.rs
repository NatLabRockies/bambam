use std::collections::HashSet;

use crate::schedule::{date::DateMapping, GtfsEdge};

/// the result of processing one GTFS archive for Compass
pub struct GtfsBundle {
    pub edges: Vec<GtfsEdge>,
    pub metadata: serde_json::Value,
    pub date_mapping: HashSet<DateMapping>,
}

impl GtfsBundle {
    /// create an empty bundle.
    pub fn empty() -> Self {
        Self {
            edges: vec![],
            metadata: serde_json::Value::Null,
            date_mapping: HashSet::new(),
        }
    }

    /// true if no GTFS edges were created or if no schedules were recorded
    /// for any edges in this GTFS bundle.
    pub fn is_empty(&self) -> bool {
        for edge in self.edges.iter() {
            if !edge.schedules.is_empty() {
                return false;
            }
        }
        true
    }
}
