use std::sync::Arc;

use routee_compass_core::model::frontier::FrontierModelService;

use crate::{model::frontier::model::GtfsFlexDepartureFrontierModel, util::zone::ZoneLookup};

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

impl FrontierModelService for GtfsFlexDepartureFrontierService {
    fn build(
        &self,
        _query: &serde_json::Value,
        _state_model: std::sync::Arc<routee_compass_core::model::state::StateModel>,
    ) -> Result<
        std::sync::Arc<dyn routee_compass_core::model::frontier::FrontierModel>,
        routee_compass_core::model::frontier::FrontierModelError,
    > {
        let model = GtfsFlexDepartureFrontierModel::new(self.lookup.clone());
        Ok(Arc::new(model))
    }
}
