use std::sync::Arc;

use crate::{
    model::constraint::service::GtfsFlexDepartureFrontierService,
    util::zone::{ZoneLookup, ZoneLookupConfig},
};

use routee_compass_core::model::constraint::{
    ConstraintModelBuilder, ConstraintModelError, ConstraintModelService,
};

pub struct GtfsFlexDepartureFrontierBuilder {}

impl ConstraintModelBuilder for GtfsFlexDepartureFrontierBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn ConstraintModelService>, ConstraintModelError> {
        let config: ZoneLookupConfig = serde_json::from_value(parameters.clone()).map_err(|e| {
            let msg = format!("failure reading config for Flex builder: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let lookup = ZoneLookup::try_from(&config).map_err(|e| {
            let msg = format!("failure building engine from config for GtfsFlex builder: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let service = GtfsFlexDepartureFrontierService::new(lookup);
        Ok(Arc::new(service))
    }
}
