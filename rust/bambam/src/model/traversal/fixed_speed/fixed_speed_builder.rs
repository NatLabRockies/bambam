use crate::model::traversal::fixed_speed::FixedSpeedModel;

use super::fixed_speed_config::FixedSpeedConfig;
use routee_compass_core::model::traversal::{
    TraversalModelBuilder, TraversalModelError, TraversalModelService,
};
use std::sync::Arc;

pub struct FixedSpeedBuilder {}

impl TraversalModelBuilder for FixedSpeedBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModelService>, TraversalModelError> {
        let config: FixedSpeedConfig = serde_json::from_value(parameters.clone()).map_err(|e| {
            TraversalModelError::BuildError(format!(
                "failure reading fixed speed traversal model configuration: {e}",
            ))
        })?;
        let service = FixedSpeedModel::new(Arc::new(config));
        Ok(Arc::new(service))
    }
}
