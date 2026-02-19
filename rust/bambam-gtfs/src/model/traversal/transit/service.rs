use std::sync::Arc;

use routee_compass_core::model::traversal::{
    TraversalModel, TraversalModelError, TraversalModelService,
};

use crate::model::traversal::transit::{
    engine::TransitTraversalEngine, model::TransitTraversalModel, query::TransitTraversalQuery,
};

pub struct TransitTraversalService {
    engine: Arc<TransitTraversalEngine>,
}

impl TransitTraversalService {
    pub fn new(engine: Arc<TransitTraversalEngine>) -> Self {
        Self { engine }
    }
}

impl TraversalModelService for TransitTraversalService {
    fn build(
        &self,
        query: &serde_json::Value,
    ) -> Result<std::sync::Arc<dyn TraversalModel>, TraversalModelError> {
        let model_query: TransitTraversalQuery =
            serde_json::from_value(query.clone()).map_err(|e| {
                TraversalModelError::BuildError(format!(
                    "failed to deserialize configuration for transit traversal model: {e}"
                ))
            })?;

        let model = TransitTraversalModel::new(
            self.engine.clone(),
            model_query.start_datetime,
            model_query.record_dwell_time.unwrap_or_default(),
        );
        Ok(Arc::new(model))
    }
}
