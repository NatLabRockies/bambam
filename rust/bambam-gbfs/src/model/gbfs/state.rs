use std::collections::HashMap;

use crate::model::gbfs::GbfsZoneRecord;

/// aggregated record to represent the rules at a given time/place for use of GBFS.
/// built from the GBFS `geofencing_rules` for all zones that intersect with the
/// current destination of the graph search, flattened according to the rules of
/// precedence: <https://gbfs.org/documentation/reference/#geofencing-rule-precedence>.
pub struct ZoneState {
    /// agency associated with this zone.
    pub system_id: String,
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

impl ZoneState {
    pub fn new(record: &GbfsZoneRecord) -> Self {
        Self {
            system_id: record.system_id.clone(),
            ride_start_allowed: record.ride_start_allowed,
            ride_end_allowed: record.ride_end_allowed,
            ride_through_allowed: record.ride_through_allowed,
            maximum_speed_kph: record.maximum_speed_kph,
            station_parking: record.station_parking,
        }
    }

    // only overwrite values if they are the default when appending a lower-tier record, in accordance
    // with the rules of precedence: <https://gbfs.org/documentation/reference/#geofencing-rule-precedence>.
    pub fn append(&mut self, record: &GbfsZoneRecord) {
        // Apply lowest index record values if an earlier record hasn't set it since we want highest precedence
        if self.ride_start_allowed {
            self.ride_start_allowed = record.ride_start_allowed;
        }
        if self.ride_end_allowed {
            self.ride_end_allowed = record.ride_end_allowed;
        }
        if self.ride_through_allowed {
            self.ride_through_allowed = record.ride_through_allowed;
        }
        if self.maximum_speed_kph.is_none() {
            self.maximum_speed_kph = record.maximum_speed_kph;
        }
        if self.station_parking {
            self.station_parking = record.station_parking;
        }
    }

    /// combines global and feature zone record "rules" as described in
    /// <https://gbfs.org/documentation/reference/#geofencing-rule-precedence>.
    ///
    /// note: GbfsZoneRecords have already been "flattened" into concrete
    /// boolean values by injecting the implicit default value "true" where
    /// a field is omitted from the source rule.
    pub fn collect_zones(intersection: Vec<&GbfsZoneRecord>) -> Vec<ZoneState> {
        let mut result: HashMap<&String, Self> = HashMap::new();

        // Sort records by feature index so that the earliest overlapping polygon takes precedence.
        let mut records: Vec<&GbfsZoneRecord> = intersection.clone();
        records.sort_by_key(|r| r.feature_index);

        // Iterate records from lowest index to highest, applying values and keeping the highest-precedence (lowest index).
        for record in records {
            result
                .entry(&record.system_id)
                .and_modify(|zone| zone.append(record))
                .or_insert(Self::new(record));
        }

        result.into_values().collect()
    }
}
