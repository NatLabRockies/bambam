use std::sync::Arc;

use super::{LtsEngine, LtsModel, LtsParams};

use routee_compass_core::model::traversal::{
    TraversalModel, TraversalModelError, TraversalModelService,
};

pub struct LtsService {
    engine: Arc<LtsEngine>,
}

impl LtsService {
    pub fn new(engine: LtsEngine) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }
}

impl TraversalModelService for LtsService {
    fn build(
        &self,
        query: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let params: LtsParams = serde_json::from_value(query.clone()).map_err(|e| {
            let msg = format!("failure reading params for Lts service: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let model = LtsModel::new(self.engine.clone(), params)?;
        Ok(Arc::new(model))
    }
}
