use chrono::TimeDelta;
use serde::{Deserialize, Serialize};

use super::ZoneId;

/// record of a GTFS-Flex travel relation, which may be
///   - Service Type 1: dst_zone_id, start_time, and end_time are None
///   - Service Type 2: start_time and end_time are None
///   - Service Type 3: no fields are None
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ZoneRecord {
    /// original trip identifier tied to this relation. retained for logging,
    /// unused in building relational graph.
    pub trip_id: String,
    /// source location associated with this trip.
    pub src_zone_id: ZoneId,
    /// destination location associated with this trip.
    pub dst_zone_id: Option<ZoneId>,
    /// time that the pickup/drop-off window begins for this trip
    #[serde(rename = "start_pickup_drop_off_window")]
    pub start_time: Option<TimeDelta>,
    /// time that the pickup/drop-off window concludes for this trip
    #[serde(rename = "end_pickup_drop_off_window")]
    pub end_time: Option<TimeDelta>,
}
