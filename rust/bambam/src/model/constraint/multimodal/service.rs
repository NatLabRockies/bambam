use std::{path::Path, sync::Arc};

use routee_compass_core::{
    model::{
        constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
        state::StateModel,
    },
    util::fs::{read_decoders, read_utils},
};

use crate::model::{
    constraint::multimodal::{
        model::MultimodalConstraintModel, MultimodalConstraintConfig,
        Constraint, MultimodalConstraintEngine,
    },
    state::{MultimodalMapping, MultimodalStateMapping},
};

pub struct MultimodalConstraintService {
    pub engine: Arc<MultimodalConstraintEngine>,
}

impl MultimodalConstraintService {
    pub fn new(
        config: MultimodalConstraintConfig,
    ) -> Result<MultimodalConstraintService, ConstraintModelError> {
        let mode_mapping = MultimodalMapping::new(&config.available_modes).map_err(|e| {
            ConstraintModelError::BuildError(format!("while building mode mapping: {e}"))
        })?;
        let route_id_to_state = match &config.route_ids_input_file {
            Some(input_file) => {
                let rmap =
                    MultimodalStateMapping::from_enumerated_category_file(Path::new(&input_file))
                        .map_err(|e| {
                        ConstraintModelError::BuildError(format!(
                            "failure building route id mapping from input file {input_file}: {e}"
                        ))
                    })?;
                Arc::new(Some(rmap))
            }
            None => Arc::new(None),
        };
        let mode_to_state = Arc::new(mode_mapping);
        let constraints = config
            .constraints
            .iter()
            .map(Constraint::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let engine = MultimodalConstraintEngine {
            mode: config.this_mode,
            constraints,
            max_trip_legs: config.max_trip_legs,
            mode_to_state,
            route_id_to_state,
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
        let model = MultimodalConstraintModel::new(self.engine.clone());
        Ok(Arc::new(model))
    }
}
