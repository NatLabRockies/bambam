use std::sync::Arc;

use super::{GbfsTraversalConfig, GbfsTraversalEngine, GbfsTraversalService};

use routee_compass_core::model::traversal::{
    TraversalModelBuilder, TraversalModelError, TraversalModelService,
};

pub struct GbfsTraversalBuilder {}

impl TraversalModelBuilder for GbfsTraversalBuilder {
    fn build(
        &self,
        value: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModelService>, TraversalModelError> {
        let config: GbfsTraversalConfig = serde_json::from_value(value.clone()).map_err(|e| {
            let msg = format!("failure reading config for GbfsTraversal builder: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let engine = GbfsTraversalEngine::try_from(config).map_err(|e| {
            let msg = format!("failure building engine from config for GbfsTraversal builder: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let service = GbfsTraversalService::new(engine);
        Ok(Arc::new(service))
    }
}
