use std::sync::Arc;

use crate::util::zone::ZoneLookup;

use super::{GtfsFlexModel, GtfsFlexParams};

use routee_compass_core::model::traversal::{
    TraversalModel, TraversalModelError, TraversalModelService,
};

pub struct GtfsFlexService {
    lookup: Arc<ZoneLookup>,
}

impl GtfsFlexService {
    pub fn new(lookup: ZoneLookup) -> Self {
        Self {
            lookup: Arc::new(lookup),
        }
    }
}

impl TraversalModelService for GtfsFlexService {
    fn build(
        &self,
        query: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let params: GtfsFlexParams = serde_json::from_value(query.clone()).map_err(|e| {
            let msg = format!("failure reading params for GtfsFlex service: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let model = GtfsFlexModel::new(self.lookup.clone(), params);
        Ok(Arc::new(model))
    }
}
