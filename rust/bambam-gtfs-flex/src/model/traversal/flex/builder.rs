use std::sync::Arc;

use super::GtfsFlexService;
use crate::util::zone::{ZoneLookup, ZoneLookupConfig};

use routee_compass_core::model::traversal::{
    TraversalModelBuilder, TraversalModelError, TraversalModelService,
};

pub struct GtfsFlexBuilder {}

impl TraversalModelBuilder for GtfsFlexBuilder {
    fn build(
        &self,
        config: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModelService>, TraversalModelError> {
        let config: ZoneLookupConfig = serde_json::from_value(config.clone()).map_err(|e| {
            let msg = format!("failure reading config for Flex builder: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let lookup = ZoneLookup::try_from(&config).map_err(|e| {
            let msg = format!("failure building engine from config for GtfsFlex builder: {e}");
            TraversalModelError::BuildError(msg)
        })?;
        let service = GtfsFlexService::new(lookup);
        Ok(Arc::new(service))
    }
}
