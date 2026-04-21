use std::sync::Arc;

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
    state::StateModel,
};

use crate::{
    model::{constraint::model::GtfsFlexDepartureConstraintModel, GtfsFlexParams},
    util::zone::ZoneLookup,
};

pub struct GtfsFlexDepartureFrontierService {
    lookup: Arc<ZoneLookup>,
}

impl GtfsFlexDepartureFrontierService {
    pub fn new(lookup: ZoneLookup) -> Self {
        Self {
            lookup: Arc::new(lookup),
        }
    }
}

impl ConstraintModelService for GtfsFlexDepartureFrontierService {
    fn build(
        &self,
        query: &serde_json::Value,
        _state_model: std::sync::Arc<StateModel>,
    ) -> Result<std::sync::Arc<dyn ConstraintModel>, ConstraintModelError> {
        let params: GtfsFlexParams = serde_json::from_value(query.clone()).map_err(|e| {
            let msg = format!("failure reading params for GtfsFlex service: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let model = GtfsFlexDepartureConstraintModel::new(self.lookup.clone(), params);
        Ok(Arc::new(model))
    }
}
