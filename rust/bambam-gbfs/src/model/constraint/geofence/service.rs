use std::sync::Arc;

use crate::model::constraint::geofence::{GeofenceConstraintEngine, GeofenceConstraintModel};

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
    state::StateModel,
};

pub struct GeofenceConstraintService {
    pub engine: Arc<GeofenceConstraintEngine>,
}

impl GeofenceConstraintService {
    pub fn new(engine: GeofenceConstraintEngine) -> GeofenceConstraintService {
        GeofenceConstraintService {
            engine: Arc::new(engine),
        }
    }
}

impl ConstraintModelService for GeofenceConstraintService {
    fn build(
        &self,
        _query: &serde_json::Value,
        _state_model: Arc<StateModel>,
    ) -> Result<Arc<dyn ConstraintModel>, ConstraintModelError> {
        Ok(Arc::new(GeofenceConstraintModel::new(self.engine.clone())))
    }
}
