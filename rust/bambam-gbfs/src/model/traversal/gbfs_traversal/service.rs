use std::sync::Arc;

use super::{GbfsTraversalEngine, GbfsTraversalModel, GbfsTraversalParams};

use routee_compass_core::model::traversal::{
    TraversalModel, TraversalModelError, TraversalModelService,
};

pub struct GbfsTraversalService {
    engine: Arc<GbfsTraversalEngine>,
}

impl GbfsTraversalService {
    pub fn new(engine: GbfsTraversalEngine) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }
}

impl TraversalModelService for GbfsTraversalService {
    fn build(
        &self,
        query: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let params: GbfsTraversalParams = serde_json::from_value(query.clone()).map_err(|e| {
            let msg = format!("failure reading params for GbfsTraversal service: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let model = GbfsTraversalModel::new(self.engine.clone(), params);
        Ok(Arc::new(model))
    }
}
