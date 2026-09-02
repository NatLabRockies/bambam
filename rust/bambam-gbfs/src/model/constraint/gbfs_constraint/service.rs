use std::sync::Arc;

use super::{GbfsConstraintEngine, GbfsConstraintModel, GbfsConstraintParams};

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
    state::StateModel,
};

pub struct GbfsConstraintService {
    engine: Arc<GbfsConstraintEngine>,
}

impl GbfsConstraintService {
    pub fn new(engine: GbfsConstraintEngine) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }
}

impl ConstraintModelService for GbfsConstraintService {
    fn build(
        &self,
        query: &serde_json::Value,
        #[allow(unused)] state_model: Arc<StateModel>,
    ) -> Result<Arc<dyn ConstraintModel>, ConstraintModelError> {
        let params: GbfsConstraintParams = serde_json::from_value(query.clone()).map_err(|e| {
            let msg = format!("failure reading params for GbfsConstraint service: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let model = GbfsConstraintModel::new(self.engine.clone(), params);
        Ok(Arc::new(model))
    }
}
