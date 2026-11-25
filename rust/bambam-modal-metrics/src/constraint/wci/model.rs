use std::sync::Arc;

use super::{WciEngine, WciParams};

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
    traversal::EdgeFrontierContext,
};

pub struct WciModel {
    pub engine: Arc<WciEngine>,
    pub params: WciParams,
}

impl WciModel {
    pub fn new(engine: Arc<WciEngine>, params: WciParams) -> Self {
        // modify this and the struct definition if additional pre-processing
        // is required during model instantiation from query parameters.
        Self { engine, params }
    }
}

impl ConstraintModel for WciModel {
    fn valid_frontier(
        &self,
        ctx: &EdgeFrontierContext,
        _state: &[StateVariable],
        _state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        self.engine
            .accept_edge_wci(ctx.edge, self.params.wci_threshold)
    }

    fn valid_edge(&self, edge: &Edge) -> Result<bool, ConstraintModelError> {
        self.engine.accept_edge_wci(edge, self.params.wci_threshold)
    }
}
