use std::{path::Path, sync::Arc};

use routee_compass_core::{
    model::traversal::{TraversalModel, TraversalModelError, TraversalModelService},
    util::fs::{read_decoders, read_utils},
};
use serde_json::Value;

use crate::model::traversal::multimodal::{
    MultimodalTraversalConfig, MultimodalTraversalModel, MultimodalTraversalQuery,
};
use bambam_core::model::state::{MultimodalMapping, MultimodalStateMapping};

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
        let model = MultimodalTraversalModel::new(
            self.config.this_mode.clone(),
            query_config.max_trip_legs,
            self.mode_enumeration.clone(),
        );
        Ok(Arc::new(model))
    }
}
