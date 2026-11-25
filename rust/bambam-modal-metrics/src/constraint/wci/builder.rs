use std::sync::Arc;

use super::{WciConfig, WciEngine, WciService};

use routee_compass_core::model::constraint::{
    ConstraintModelBuilder, ConstraintModelError, ConstraintModelService,
};

pub struct WciBuilder {}

impl ConstraintModelBuilder for WciBuilder {
    fn build(
        &self,
        value: &serde_json::Value,
    ) -> Result<Arc<dyn ConstraintModelService>, ConstraintModelError> {
        let config: WciConfig = serde_json::from_value(value.clone()).map_err(|e| {
            let msg = format!("failure reading config for Wci builder: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let engine = WciEngine::try_from(config).map_err(|e| {
            let msg = format!("failure building engine from config for Wci builder: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let service = WciService::new(engine);
        Ok(Arc::new(service))
    }
}
