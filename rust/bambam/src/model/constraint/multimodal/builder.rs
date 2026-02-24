use std::sync::Arc;

use routee_compass_core::model::constraint::{
    ConstraintModelBuilder, ConstraintModelError, ConstraintModelService,
};

use crate::model::constraint::multimodal::{
    MultimodalConstraintConfig, MultimodalConstraintService,
};

pub struct MultimodalConstraintBuilder {}

impl ConstraintModelBuilder for MultimodalConstraintBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn ConstraintModelService>, ConstraintModelError> {
        let config: MultimodalConstraintConfig = serde_json::from_value(parameters.clone())
            .map_err(|e| {
                ConstraintModelError::BuildError(format!(
                    "while reading multimodal frontier model configuration: {e}"
                ))
            })?;
        let service = MultimodalConstraintService::new(config)?;
        Ok(Arc::new(service))
    }
}
