use std::{path::Path, sync::Arc};

use routee_compass_core::{
    model::traversal::{TraversalModel, TraversalModelError, TraversalModelService},
    util::fs::{read_decoders, read_utils},
};
use serde_json::Value;

use crate::model::{
    state::{MultimodalMapping, MultimodalStateMapping},
    traversal::multimodal::{
        MultimodalTraversalConfig, MultimodalTraversalModel, MultimodalTraversalQuery,
    },
};

pub struct MultimodalTraversalService {
    pub config: MultimodalTraversalConfig,
    pub mode_to_state: Arc<MultimodalStateMapping>,
    pub route_id_to_state: Arc<Option<MultimodalStateMapping>>,
}

impl MultimodalTraversalService {
    pub fn new(
        config: MultimodalTraversalConfig,
    ) -> Result<MultimodalTraversalService, TraversalModelError> {
        let mode_to_state = Arc::new(MultimodalMapping::new(&config.available_modes)?);
        let route_id_to_state = match &config.route_ids_input_file {
            Some(input_file) => {
                let rmap =
                    MultimodalStateMapping::from_enumerated_category_file(Path::new(&input_file))?;
                Arc::new(Some(rmap))
            }
            None => Arc::new(None),
        };
        let result = MultimodalTraversalService {
            config,
            mode_to_state,
            route_id_to_state,
        };
        Ok(result)
    }
}

impl TraversalModelService for MultimodalTraversalService {
    fn build(&self, query: &Value) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let query_config: MultimodalTraversalQuery = serde::Deserialize::deserialize(query).map_err(|e| TraversalModelError::BuildError(format!("failure while deserializing query in MultimodalTraversalService for {}-mode: {e}", self.config.this_mode)))?;
        let mode_to_state = match query_config.available_modes {
            Some(available_modes) => Arc::new(MultimodalMapping::new(&available_modes)?),
            None => self.mode_to_state.clone(),
        };
        let route_id_to_state = match query_config.available_route_ids {
            Some(available_route_ids) => {
                Arc::new(Some(MultimodalMapping::new(&available_route_ids)?))
            }
            None => self.route_id_to_state.clone(),
        };
        let model = MultimodalTraversalModel::new(
            self.config.this_mode.clone(),
            query_config.max_trip_legs,
            mode_to_state,
            route_id_to_state,
        );
        Ok(Arc::new(model))
    }
}
