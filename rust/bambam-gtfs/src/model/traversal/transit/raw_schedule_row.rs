use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// record type storing a single scheduled departure and arrival
/// within a route.
#[derive(Debug, Deserialize, Serialize)]
pub struct RawScheduleRow {
    pub edge_id: usize,
    /// fully-qualified route id
    pub fully_qualified_id: String,
    pub src_departure_time: NaiveDateTime,
    pub dst_arrival_time: NaiveDateTime,
}
