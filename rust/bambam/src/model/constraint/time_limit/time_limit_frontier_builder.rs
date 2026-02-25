use crate::model::constraint::time_limit::{TimeLimitConfig, TimeLimitConstraintConfig};

use super::time_limit_frontier_service::TimeLimitConstraintService;
use routee_compass_core::model::{
    constraint::{ConstraintModelBuilder, ConstraintModelError, ConstraintModelService},
    unit::TimeUnit,
};
use std::sync::Arc;

pub struct TimeLimitConstraintBuilder {}

impl ConstraintModelBuilder for TimeLimitConstraintBuilder {
    fn build(
        &self,
        config: &serde_json::Value,
    ) -> Result<Arc<dyn ConstraintModelService>, ConstraintModelError> {
        let conf: TimeLimitConstraintConfig =
            serde_json::from_value(config.clone()).map_err(|e| {
                ConstraintModelError::BuildError(format!(
                    "failure reading isochrone frontier model configuration: {e}"
                ))
            })?;
        let model = TimeLimitConstraintService::new(&conf);
        Ok(Arc::new(model))
    }
}
