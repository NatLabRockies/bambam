use crate::model::gbfs::GbfsZoneRecord;

/// aggregated record to represent the rules at a given time/place for use of GBFS.
/// built from the GBFS `geofencing_rules` for all zones that intersect with the
/// current destination of the graph search, flattened according to the rules of
/// precedence: <https://gbfs.org/documentation/reference/#geofencing-rule-precedence>.
pub struct ZoneRules {
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

impl ZoneRules {
    /// combines global and feature zone record "rules" as described in
    /// <https://gbfs.org/documentation/reference/#geofencing-rule-precedence>.
    ///
    /// note: GbfsZoneRecords have already been "flattened" into concrete
    /// boolean values by injecting the implicit default value "true" where
    /// a field is omitted from the source rule.
    pub fn from_records<'a, 'b>(
        global: Option<&'a GbfsZoneRecord>,
        intersection: Box<dyn Iterator<Item = &'b GbfsZoneRecord> + 'b>,
    ) -> Option<ZoneRules> {
        let mut ride_start_allowed = global.map(|g| g.ride_start_allowed);
        let mut ride_end_allowed = global.map(|g| g.ride_end_allowed);
        let mut ride_through_allowed = global.map(|g| g.ride_through_allowed);
        let mut maximum_speed_kph = global.and_then(|g| g.maximum_speed_kph);
        let mut station_parking = global.map(|g| g.station_parking);

        // Sort records by feature index so that the earliest overlapping polygon takes precedence.
        let mut records: Vec<&GbfsZoneRecord> = intersection.collect();
        records.sort_by_key(|r| r.feature_index.unwrap_or(usize::MAX));

        // Iterate records from lowest index to highest, applying values and keeping the highest-precedence (lowest index).
        for record in records {
            // Apply lowest index record values if an earlier record hasn't set it since we want highest precedence
            if ride_start_allowed.is_none() {
                ride_start_allowed = Some(record.ride_start_allowed);
            }
            if ride_end_allowed.is_none() {
                ride_end_allowed = Some(record.ride_end_allowed);
            }
            if ride_through_allowed.is_none() {
                ride_through_allowed = Some(record.ride_through_allowed);
            }
            if maximum_speed_kph.is_none() {
                maximum_speed_kph = record.maximum_speed_kph;
            }
            if station_parking.is_none() {
                station_parking = Some(record.station_parking);
            }
        }

        Some(ZoneRules {
            ride_start_allowed: ride_start_allowed.unwrap_or(true),
            ride_end_allowed: ride_end_allowed.unwrap_or(true),
            ride_through_allowed: ride_through_allowed.unwrap_or(true),
            maximum_speed_kph,
            station_parking: station_parking.unwrap_or(true),
        })
    }
}
