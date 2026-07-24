use std::sync::Arc;

use super::{GbfsConstraintEngine, GbfsConstraintParams};

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
    traversal::EdgeFrontierContext,
};

pub struct GbfsConstraintModel {
    pub engine: Arc<GbfsConstraintEngine>,
    pub params: GbfsConstraintParams,
}

impl GbfsConstraintModel {
    pub fn new(engine: Arc<GbfsConstraintEngine>, params: GbfsConstraintParams) -> Self {
        // modify this and the struct definition if additional pre-processing
        // is required during model instantiation from query parameters.
        Self { engine, params }
    }
}

impl ConstraintModel for GbfsConstraintModel {
    fn valid_frontier(
        &self,
        _ctx: &EdgeFrontierContext,
        _state: &[StateVariable],
        _state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        todo!()
    }

    fn valid_edge(&self, _edge: &Edge) -> Result<bool, ConstraintModelError> {
        todo!()
    }
}
