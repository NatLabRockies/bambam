use routee_compass_core::model::unit::SpeedUnit;
use serde::{Deserialize, Serialize};
use uom::si::f64::Velocity;

use crate::model::traversal::transit::schedule_loading_policy::ScheduleLoadingPolicy;

/// configures a transit traversal model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "schedule_format", rename_all = "snake_case")]
pub enum TransitTraversalConfig {
    /// Pre-compiled CSR EdgeTimetable in rkyv binary snapshot format
    Rkyv {
        /// Path to the serialized EdgeTimetable (.rkyv) file
        timetable_input_file: String, // (or alias = "edges_schedules_input_file" for backwards compatibility)
        /// Optional walking speed `(value, unit)` used to compute the vertex-to-stop boarding delay
        #[serde(default)]
        walk_speed: WalkSpeed,
    },
    /// Raw CSV schedule + metadata workflow
    Csv {
        /// edges-schedules file path from gtfs preprocessing (.csv.gz)
        edges_schedules_input_file: String,
        /// metadata file path from gtfs preprocessing (.json)
        gtfs_metadata_input_file: String,
        /// policy by which to prune departures when reading schedules
        #[serde(default)]
        schedule_loading_policy: ScheduleLoadingPolicy,
        /// if provided, overrides the metadata entry for fully-qualified
        /// route ids, in the case of running multiple transit models simultaneously.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route_ids_input_file: Option<String>,
        /// optional walking speed `(value, unit)` used to compute the vertex-to-stop boarding delay
        #[serde(default)]
        walk_speed: WalkSpeed,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct WalkSpeed {
    value: f64,
    unit: SpeedUnit,
}

impl WalkSpeed {
    pub fn to_uom(&self) -> Velocity {
        self.unit.to_uom(self.value)
    }
}

impl Default for WalkSpeed {
    fn default() -> Self {
        Self {
            value: 5.0,
            unit: SpeedUnit::KPH,
        }
    }
}
