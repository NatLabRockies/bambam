use crate::model::traversal::fixed_speed::FixedSpeedConfig;
use bambam_core::model::bambam_state;
use chrono::format::Fixed;
use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::{Edge, Vertex},
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{TraversalModel, TraversalModelError, TraversalModelService},
        unit::SpeedUnit,
    },
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uom::{si::f64::Velocity, ConstZero};

#[derive(Clone, Debug)]
pub struct FixedSpeedModel {
    pub config: Arc<FixedSpeedConfig>,
    /// speed value to write on each state vector
    pub speed: Velocity,
    /// name of state feature where these speed values are assigned
    pub fieldname: String,
}

impl FixedSpeedModel {
    pub fn new(config: Arc<FixedSpeedConfig>) -> FixedSpeedModel {
        let speed = config.speed_unit.to_uom(config.speed);
        FixedSpeedModel {
            config: config.clone(),
            speed,
            fieldname: bambam_state::EDGE_SPEED.to_string(),
        }
    }
}

impl TraversalModelService for FixedSpeedModel {
    fn build(
        &self,
        _query: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let model: Arc<dyn TraversalModel> = Arc::new(self.clone());
        Ok(model)
    }
}

impl TraversalModel for FixedSpeedModel {
    fn name(&self) -> String {
        format!("Fixed Speed Model ({})", self.config.name)
    }

    fn input_features(&self) -> Vec<InputFeature> {
        vec![]
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        vec![(
            self.fieldname.clone(),
            StateVariableConfig::Speed {
                accumulator: false,
                initial: Velocity::ZERO,
                output_unit: Some(self.config.speed_unit),
            },
        )]
    }

    fn traverse_edge(
        &self,
        trajectory: (&Vertex, &Edge, &Vertex),
        state: &mut Vec<StateVariable>,
        tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        state_model.set_speed(state, &self.fieldname, &self.speed)?;
        Ok(())
    }

    fn estimate_traversal(
        &self,
        od: (&Vertex, &Vertex),
        state: &mut Vec<StateVariable>,
        tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        state_model.set_speed(state, &self.fieldname, &self.speed)?;
        Ok(())
    }
}
