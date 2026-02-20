use std::sync::Arc;

use crate::model::constraint::geofence::GeofenceConstraintEngine;

use super::{GeofenceConstraintConfig, GeofenceConstraintService};
use routee_compass_core::model::constraint::{
    ConstraintModelBuilder, ConstraintModelError, ConstraintModelService,
};
use routee_compass_core::util::geo::PolygonalRTree;
pub struct GeofenceConstraintBuilder {}

impl ConstraintModelBuilder for GeofenceConstraintBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn ConstraintModelService>, ConstraintModelError> {
        let config: GeofenceConstraintConfig = serde_json::from_value(parameters.clone())
            .map_err(|e| ConstraintModelError::BuildError(e.to_string()))?;
        let rtree = PolygonalRTree::new(vec![]).map_err(ConstraintModelError::BuildError)?;
        let engine = GeofenceConstraintEngine::new(config, rtree);
        let service = GeofenceConstraintService::new(engine);
        Ok(Arc::new(service))
    }
}
