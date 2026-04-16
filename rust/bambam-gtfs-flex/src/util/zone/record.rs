use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

use crate::util::zone::ZoneSchedule;

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
    #[serde(rename = "origin_zone")]
    pub src_zone_id: ZoneId,
    /// destination location associated with this trip.
    #[serde(rename = "destination_zone")]
    pub dst_zone_id: Option<ZoneId>,
    /// time that the pickup/drop-off window begins for this trip
    #[serde(rename = "start_pickup_drop_off_window")]
    pub start_time: Option<NaiveTime>,
    /// time that the pickup/drop-off window concludes for this trip
    #[serde(rename = "end_pickup_drop_off_window")]
    pub end_time: Option<NaiveTime>,
}

impl ZoneRecord {
    /// gets the time range from the row. treats rows with missing start_time or end_time
    /// as open intervals for the current date.
    pub fn get_zone_schedule(&self) -> Option<ZoneSchedule> {
        let schedule_internal = match (self.start_time, self.end_time) {
            (None, None) => None,
            (None, Some(end)) => Some((NaiveTime::MIN, end)),
            (Some(start), None) => {
                let end = NaiveTime::from_hms_opt(23, 59, 59)?;
                Some((start, end))
            }
            (Some(start), Some(end)) => Some((start, end)),
        };
        schedule_internal.map(|(s, e)| ZoneSchedule::new(s, e))
    }
}
