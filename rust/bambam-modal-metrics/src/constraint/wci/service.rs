use std::sync::Arc;

use super::{WciEngine, WciModel, WciParams};

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
    state::StateModel,
};

pub struct WciService {
    engine: Arc<WciEngine>,
}

impl WciService {
    pub fn new(engine: WciEngine) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }
}

impl ConstraintModelService for WciService {
    fn build(
        &self,
        query: &serde_json::Value,
        #[allow(unused)] state_model: Arc<StateModel>,
    ) -> Result<Arc<dyn ConstraintModel>, ConstraintModelError> {
        let params: WciParams = serde_json::from_value(query.clone()).map_err(|e| {
            let msg = format!("failure reading params for Wci service: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let model = WciModel::new(self.engine.clone(), params);
        Ok(Arc::new(model))
    }
}
