use std::sync::Arc;

use crate::model::traversal::transit::{
    config::TransitTraversalConfig, engine::TransitTraversalEngine,
    service::TransitTraversalService,
};
use routee_compass_core::model::traversal::{
    TraversalModelBuilder, TraversalModelError, TraversalModelService,
};

pub struct TransitTraversalBuilder {}

impl TraversalModelBuilder for TransitTraversalBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<std::sync::Arc<dyn TraversalModelService>, TraversalModelError> {
        let config: TransitTraversalConfig =
            serde_json::from_value(parameters.clone()).map_err(|e| {
                TraversalModelError::BuildError(format!(
                    "failed to read transit_traversal configuration: {e}"
                ))
            })?;

        let engine = TransitTraversalEngine::try_from(config)?;
        let service = TransitTraversalService::new(Arc::new(engine));

        Ok(Arc::new(service))
    }
}
