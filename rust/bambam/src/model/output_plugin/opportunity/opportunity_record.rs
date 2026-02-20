use bambam_core::model::{
    bambam_field, bambam_state, output_plugin::opportunity::OpportunityOrientation, TimeBin,
};
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::model::{
    state::{StateModel, StateVariable},
    unit::TimeUnit,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uom::si::f64::Time;

/// properties of accessing some activity type from a grid cell origin location. comes in two flavors:
///
///   1. Aggregate    - zonal/isochrone access to a type of activity
///   2. Disaggregate - access data for exactly one opportunity
///
/// the properties of this opportunity access influence the modal intensities, modal coefficients,
/// and activity frequencies selected for computing an access metric.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OpportunityRecord {
    Aggregate {
        activity_type: String,
        geometry: geo::Geometry<f32>,
        time_bin: TimeBin,
        count: f64,
    },
    Disaggregate {
        id: String,
        activity_type: String,
        opportunity_orientation: OpportunityOrientation,
        geometry: geo::Geometry<f32>,
        state: Vec<StateVariable>,
    },
}

impl OpportunityRecord {
    pub fn get_json_path(&self) -> Vec<String> {
        match self {
            OpportunityRecord::Aggregate { time_bin, .. } => {
                vec![bambam_field::TIME_BINS.to_string(), time_bin.key()]
            }
            OpportunityRecord::Disaggregate { id, .. } => {
                vec![bambam_field::OPPORTUNITIES.to_string(), id.to_string()]
            }
        }
    }

    pub fn get_time(&self, state_model: Arc<StateModel>) -> Result<Time, OutputPluginError> {
        match self {
            Self::Disaggregate { state, .. } => {
                // time comes from the trip travel time taken to reach this point
                state_model.get_time(state, bambam_state::TRIP_TIME)
                    .map_err(|e| OutputPluginError::OutputPluginFailed(format!("with disaggregate opportunity record, could not find trip time due to: {e}")))
            }
            Self::Aggregate { time_bin, .. } => {
                // time comes from the isochrone bin
                Ok(time_bin.max_time())
            }
        }
    }
    pub fn get_activity_type(&self) -> &str {
        match self {
            Self::Aggregate { activity_type, .. } => activity_type,
            Self::Disaggregate { activity_type, .. } => activity_type,
        }
    }

    pub fn get_count(&self) -> f64 {
        match self {
            OpportunityRecord::Aggregate { count, .. } => *count,
            OpportunityRecord::Disaggregate { .. } => 1.0,
        }
    }

    pub fn get_geometry(&self) -> &geo::Geometry<f32> {
        match self {
            Self::Aggregate { geometry, .. } => geometry,
            Self::Disaggregate { geometry, .. } => geometry,
        }
    }

    pub fn get_opportunity_orientation(&self) -> Option<&OpportunityOrientation> {
        match self {
            Self::Aggregate { .. } => None,
            Self::Disaggregate {
                opportunity_orientation,
                ..
            } => Some(opportunity_orientation),
        }
    }
}
