use std::sync::Arc;

use super::GtfsFlexModel;
use crate::model::GtfsFlexParams;
use crate::util::zone::{ZoneId, ZoneLookup};

use bambam_core::model::state::CategoricalMapping;
use routee_compass_core::model::traversal::{
    TraversalModel, TraversalModelError, TraversalModelService,
};

pub struct GtfsFlexService {
    lookup: Arc<ZoneLookup>,
    mapping: Arc<CategoricalMapping<ZoneId, i64>>,
}

impl GtfsFlexService {
    pub fn new(lookup: ZoneLookup) -> Result<Self, TraversalModelError> {
        let mut zone_ids: Vec<_> = lookup.graph.keys().cloned().collect();
        zone_ids.dedup();
        let mapping = CategoricalMapping::new(&zone_ids)?;
        Ok(Self {
            lookup: Arc::new(lookup),
            mapping: Arc::new(mapping),
        })
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
        let model = GtfsFlexModel::new(self.lookup.clone(), self.mapping.clone(), params);
        Ok(Arc::new(model))
    }
}
