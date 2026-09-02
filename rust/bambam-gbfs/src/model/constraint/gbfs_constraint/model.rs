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
        ctx: &EdgeFrontierContext,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        self.engine
            .check_valid(ctx.dst, state, state_model, self.params.start_time)
            .map_err(|e| {
                let msg = format!("failure running GBFS constraint model: {e}");
                ConstraintModelError::ConstraintModelError(msg)
            })
    }

    fn valid_edge(&self, _edge: &Edge) -> Result<bool, ConstraintModelError> {
        Ok(true)
    }
}
