use routee_compass_core::model::unit::SpeedUnit;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GbfsTraversalConfig {
    /// output of bambam-gbfs CLI import process, contains a record of the
    /// identifier, optional start/end times for service, and traversal ruleset
    /// for default vehicles (trips without a VehicleTripId).
    ///
    /// see [crate::model::gbfs::GbfsZoneRecord]
    pub zones_input_file: String,
    /// output of bambam-gbfs CLI import process, contains zonal geometries
    /// with matching indices to the zones input file.
    pub geometries_input_file: String,
    /// fully-qualified identifiers for each record with matching index to zone + geometry files.
    pub zone_ids_input_file: String,
    /// speed to use for GBFS trips. can be limited by zone-specific max speeds.
    pub default_speed: DefaultSpeed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DefaultSpeed {
    pub speed: f64,
    pub speed_unit: SpeedUnit,
}
