use std::sync::Arc;

use super::{LtsConfig, LtsEngine, LtsService};

use routee_compass_core::model::traversal::{
    TraversalModelBuilder, TraversalModelError, TraversalModelService,
};

pub struct LtsBuilder {}

impl TraversalModelBuilder for LtsBuilder {
    fn build(
        &self,
        value: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModelService>, TraversalModelError> {
        let config: LtsConfig = serde_json::from_value(value.clone()).map_err(|e| {
            let msg = format!("failure reading config for Lts builder: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let engine = LtsEngine::try_from(config).map_err(|e| {
            let msg = format!("failure building engine from config for Lts builder: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let service = LtsService::new(engine);
        Ok(Arc::new(service))
    }
}
