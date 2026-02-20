use std::sync::Arc;

use routee_compass_core::model::constraint::{
    ConstraintModelBuilder, ConstraintModelError, ConstraintModelService,
};
use routee_compass_core::util::geo::PolygonalRTree;

use super::{BoardingConstraintConfig, BoardingConstraintEngine, BoardingConstraintService};

pub struct BoardingConstraintBuilder {}

impl ConstraintModelBuilder for BoardingConstraintBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn ConstraintModelService>, ConstraintModelError> {
        let config: BoardingConstraintConfig = serde_json::from_value(parameters.clone())
            .map_err(|e| ConstraintModelError::BuildError(e.to_string()))?;
        let rtree = PolygonalRTree::new(vec![]).map_err(ConstraintModelError::BuildError)?;
        let engine = BoardingConstraintEngine::new(config, rtree);
        let service = BoardingConstraintService::new(engine);
        Ok(Arc::new(service))
    }
}
