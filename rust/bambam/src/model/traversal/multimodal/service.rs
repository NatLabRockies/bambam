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
    pub mode_enumeration: Arc<MultimodalStateMapping>,
}

impl MultimodalTraversalService {
    pub fn new(
        config: MultimodalTraversalConfig,
    ) -> Result<MultimodalTraversalService, TraversalModelError> {
        let mode_enumeration = Arc::new(MultimodalMapping::new(&config.available_modes)?);
        let result = MultimodalTraversalService {
            config,
            mode_enumeration,
        };
        Ok(result)
    }
}

impl TraversalModelService for MultimodalTraversalService {
    fn build(&self, query: &Value) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let query_config: MultimodalTraversalQuery = serde::Deserialize::deserialize(query).map_err(|e| TraversalModelError::BuildError(format!("failure while deserializing query in MultimodalTraversalService for {}-mode: {e}", self.config.this_mode)))?;
        let mode_to_state = match query_config.available_modes {
            Some(available_modes) => Arc::new(MultimodalMapping::new(&available_modes)?),
            None => self.mode_enumeration.clone(),
        };
        let model = MultimodalTraversalModel::new(
            self.config.this_mode.clone(),
            query_config.max_trip_legs,
            mode_to_state,
        );
        Ok(Arc::new(model))
    }
}
