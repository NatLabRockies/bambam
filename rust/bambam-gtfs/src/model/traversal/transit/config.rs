// Questions
// - Should the engine create the edges in compass? No
// - If we are already in the same route, should we make transit_boarding_time 0 but still the travel time = dst_arrival - current_time
// - If Schedules = Box<[Schedule]>, how do we access the correct schedule if I have an edge_id? edge_id is usize

use serde::{Deserialize, Serialize};

use crate::model::traversal::transit::schedule_loading_policy::ScheduleLoadingPolicy;

#[derive(Debug, Serialize, Deserialize)]
pub struct TransitTraversalConfig {
    /// edges-schedules file path from gtfs preprocessing
    pub edges_schedules_input_file: String,
    /// metadata file path from gtfs preprocessing
    pub gtfs_metadata_input_file: String,
    /// policy by which to prune departures when reading schedules
    pub schedule_loading_policy: ScheduleLoadingPolicy,
    /// if provided, overrides the metadata entry for fully-qualified
    /// route ids, in the case of running multiple transit models simultaneously.
    pub route_ids_input_file: Option<String>,
}
