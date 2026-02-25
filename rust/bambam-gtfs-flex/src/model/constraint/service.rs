use std::sync::Arc;

use routee_compass_core::model::constraint::ConstraintModelService;

use crate::{model::constraint::model::GtfsFlexDepartureConstraintModel, util::zone::ZoneLookup};

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
        _query: &serde_json::Value,
        _state_model: std::sync::Arc<routee_compass_core::model::state::StateModel>,
    ) -> Result<
        std::sync::Arc<dyn routee_compass_core::model::constraint::ConstraintModel>,
        routee_compass_core::model::constraint::ConstraintModelError,
    > {
        let model = GtfsFlexDepartureConstraintModel::new(self.lookup.clone());
        Ok(Arc::new(model))
    }
}
