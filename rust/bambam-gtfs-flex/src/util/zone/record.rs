use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

use super::ZoneId;
use crate::util::zone::ZoneSchedule;

/// a valid origin-destination zone pair for a trip
#[derive(Debug, Serialize, Deserialize)]
pub struct ZoneRecord {
    pub agency_id: String,
    pub feed: String,
    pub requested_date: String,
    pub trip_id: String,
    pub origin_zone: ZoneId,
    pub start_pickup_drop_off_window: Option<NaiveTime>,
    pub end_pickup_drop_off_window: Option<NaiveTime>,
    pub destination_zone: ZoneId,
}

/// geometry WKT for the fully-qualified [ZoneId].
#[derive(Debug, Serialize, Deserialize)]
pub struct ZoneGeometry {
    pub zone_id: ZoneId,
    pub geometry: String,
}

/// record of a GTFS-Flex travel relation, which may be
///   - Service Type 1: dst_zone_id, start_time, and end_time are None
///   - Service Type 2: start_time and end_time are None
///   - Service Type 3: no fields are None
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ZonalRelationRecord {
    /// fully-qualified id for the source location associated with this trip.
    #[serde(rename = "origin_zone")]
    pub src_zone_id: ZoneId,
    /// fully-qualified id for the destination location associated with this trip.
    #[serde(rename = "destination_zone")]
    pub dst_zone_id: Option<ZoneId>,
    /// time that the pickup/drop-off window begins for this trip
    #[serde(rename = "start_pickup_drop_off_window")]
    pub start_time: Option<NaiveTime>,
    /// time that the pickup/drop-off window concludes for this trip
    #[serde(rename = "end_pickup_drop_off_window")]
    pub end_time: Option<NaiveTime>,
}

impl ZonalRelationRecord {
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
