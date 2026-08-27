use std::sync::Arc;

use super::{GbfsConstraintConfig, GbfsConstraintEngine, GbfsConstraintService};

use routee_compass_core::model::constraint::{
    ConstraintModelBuilder, ConstraintModelError, ConstraintModelService,
};

pub struct GbfsConstraintBuilder {}

impl ConstraintModelBuilder for GbfsConstraintBuilder {
    fn build(
        &self,
        value: &serde_json::Value,
    ) -> Result<Arc<dyn ConstraintModelService>, ConstraintModelError> {
        let config: GbfsConstraintConfig = serde_json::from_value(value.clone()).map_err(|e| {
            let msg = format!("failure reading config for GbfsConstraint builder: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let engine = GbfsConstraintEngine::try_from(config).map_err(|e| {
            let msg =
                format!("failure building engine from config for GbfsConstraint builder: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let service = GbfsConstraintService::new(engine);
        Ok(Arc::new(service))
    }
}
