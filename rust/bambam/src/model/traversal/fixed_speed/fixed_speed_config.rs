use crate::model::traversal::fixed_speed::FixedSpeedModel;
use routee_compass_core::model::{
    network::{Edge, Vertex},
    state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
    traversal::{TraversalModel, TraversalModelError, TraversalModelService},
    unit::SpeedUnit,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FixedSpeedConfig {
    /// name of mode associated with this type of travel.
    pub name: String,
    /// fixed speed to apply
    pub speed: f64,
    /// speed unit for the fixed speed value
    pub speed_unit: SpeedUnit,
}
