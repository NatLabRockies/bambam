use routee_compass_core::model::state::StateVariable;
use serde::{Deserialize, Serialize};

/// activity counts and vehicle state observed when reaching a destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationOpportunity {
    /// opportunity counts for this location
    pub counts: Vec<f64>,
    /// vehicle state when this location was reached
    pub state: Vec<StateVariable>,
}
