use std::{path::Path, sync::Arc};

use routee_compass_core::{
    model::{
        constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
        state::StateModel,
    },
    util::fs::{read_decoders, read_utils},
};

use crate::model::constraint::multimodal::{
    model::MultimodalConstraintModel, Constraint, MultimodalConstraintConfig,
    MultimodalConstraintEngine, MultimodalConstraintModelQuery,
};
use bambam_core::model::state::{CategoricalMapping, CategoricalStateMapping};

pub struct MultimodalConstraintService {
    pub engine: Arc<MultimodalConstraintEngine>,
}

impl MultimodalConstraintService {
    pub fn new(
        config: MultimodalConstraintConfig,
    ) -> Result<MultimodalConstraintService, ConstraintModelError> {
        let mode_mapping = CategoricalMapping::new(&config.available_modes).map_err(|e| {
            ConstraintModelError::BuildError(format!("while building mode mapping: {e}"))
        })?;
        let mode_to_state = Arc::new(mode_mapping);
        let engine = MultimodalConstraintEngine {
            mode: config.this_mode,
            mode_to_state,
        };
        let service = MultimodalConstraintService {
            engine: Arc::new(engine),
        };
        Ok(service)
    }
}

impl ConstraintModelService for MultimodalConstraintService {
    fn build(
        &self,
        query: &serde_json::Value,
        state_model: Arc<StateModel>,
    ) -> Result<Arc<dyn ConstraintModel>, ConstraintModelError> {
        let model_config: MultimodalConstraintModelQuery = serde_json::from_value(query.clone())
            .map_err(|e| {
                ConstraintModelError::BuildError(format!(
                    "while reading query for multimodal constraint model, {e}"
                ))
            })?;
        let constraints = model_config.build_constraints()?;

        let model = MultimodalConstraintModel::new(
            self.engine.clone(),
            constraints,
            model_config.max_trip_legs,
        );
        Ok(Arc::new(model))
    }
}
