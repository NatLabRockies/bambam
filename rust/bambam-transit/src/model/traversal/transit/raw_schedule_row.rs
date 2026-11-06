use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uom::si::f64::Length;

/// record type storing a single scheduled departure and arrival
/// within a route.
#[derive(Debug, Deserialize, Serialize)]
pub struct RawScheduleRow {
    pub edge_id: usize,
    /// fully-qualified route id
    pub fully_qualified_id: String,
    pub src_departure_time: NaiveDateTime,
    pub dst_arrival_time: NaiveDateTime,
    /// walking distance from the source road vertex to the boarding GTFS stop.
    #[serde(default)]
    pub src_walk_distance: Length,
    /// walking distance from the alighting GTFS stop to the destination road vertex.
    #[serde(default)]
    pub dst_walk_distance: Length,
}
