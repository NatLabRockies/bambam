// Questions
// - Should the engine create the edges in compass? No
// - If we are already in the same route, should we make transit_boarding_time 0 but still the travel time = dst_arrival - current_time
// - If Schedules = Box<[Schedule]>, how do we access the correct schedule if I have an edge_id? edge_id is usize

use routee_compass_core::model::unit::SpeedUnit;
use serde::{Deserialize, Serialize};
use uom::si::f64::Velocity;

use crate::model::traversal::transit::schedule_loading_policy::ScheduleLoadingPolicy;

#[derive(Debug, Serialize, Deserialize)]
pub struct TransitTraversalConfig {
    /// edges-schedules file path from gtfs preprocessing (.csv.gz or .rkyv)
    pub edges_schedules_input_file: String,
    /// metadata file path from gtfs preprocessing
    pub gtfs_metadata_input_file: String,
    /// policy by which to prune departures when reading schedules
    pub schedule_loading_policy: ScheduleLoadingPolicy,
    /// if provided, overrides the metadata entry for fully-qualified
    /// route ids, in the case of running multiple transit models simultaneously.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_ids_input_file: Option<String>,
    /// optional walking speed `(value, unit)` used to compute the vertex-to-stop boarding delay
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walk_speed: Option<WalkSpeed>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WalkSpeed {
    value: f64,
    unit: SpeedUnit,
}

impl WalkSpeed {
    pub fn to_uom(&self) -> Velocity {
        self.unit.to_uom(self.value)
    }
}
