use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{app::download::ZoneConstraints, model::gbfs::ops};

/// a composite of SystemInformation and GeofencingZone attributes along with a
/// globally-unique identifier for this zone. the geometry is stored elsewhere.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GbfsZoneRecord {
    /// a globally-unique zone identifier that combines the system id and zone index
    pub fq_id: String,
    /// GBFS SystemInformation.system_id value.
    pub system_id: String,
    /// index of the geojson feature associated with this zone. if not provided,
    /// this record is the global zone record.
    pub feature_index: usize,
    /// optional start time for using this zone
    pub start: Option<DateTime<Utc>>,
    /// optional end time for using this zone
    pub end: Option<DateTime<Utc>>,
    /// Is the ride allowed to start in this zone?
    pub ride_start_allowed: bool,
    /// Is the ride allowed to end in this zone?
    pub ride_end_allowed: bool,
    /// Is the ride allowed to travel through this zone?
    pub ride_through_allowed: bool,
    /// What is the maximum speed allowed, in kilometers per hour?
    pub maximum_speed_kph: Option<i32>,
    /// Can vehicles only be parked at stations defined in [station_information] within this geofence zone?
    pub station_parking: bool,
}

impl GbfsZoneRecord {
    /// create a record for a zone, including its identifiers and its constraint set.
    /// if a given boolean constraint is found to be None, apply a permissive rule.
    pub fn new(
        system_id: String,
        feature_index: usize,
        start: Option<String>,
        end: Option<String>,
        zone_constraints: ZoneConstraints,
    ) -> Result<Self, String> {
        let fq_id = ops::fully_qualified_zone_id(&system_id, feature_index);
        let start = to_datetime(start)?;
        let end = to_datetime(end)?;
        let result = Self {
            fq_id,
            system_id,
            feature_index,
            start,
            end,
            ride_start_allowed: zone_constraints.ride_start_allowed.unwrap_or(true),
            ride_end_allowed: zone_constraints.ride_end_allowed.unwrap_or(true),
            ride_through_allowed: zone_constraints.ride_through_allowed.unwrap_or(true),
            maximum_speed_kph: zone_constraints.maximum_speed_kph,
            station_parking: zone_constraints.station_parking.unwrap_or(true),
        };
        Ok(result)
    }
}

fn to_datetime(value: Option<String>) -> Result<Option<DateTime<Utc>>, String> {
    match value {
        None => Ok(None),
        Some(dt_string) => {
            let dt = DateTime::parse_from_rfc3339(&dt_string)
                .map_err(|e| format!("unable to parse date {dt_string}: {e}"))?;
            Ok(Some(dt.to_utc()))
        }
    }
}
