use std::sync::Arc;

use routee_compass_core::model::constraint::ConstraintModel;

use crate::model::constraint::geofence::GeofenceConstraintEngine;

/// looks up a geofence by agency id to test whether an edge traversal
/// does not exit the region supported by this GBFS travel mode.
pub struct GeofenceConstraintModel {
    pub engine: Arc<GeofenceConstraintEngine>,
}

impl GeofenceConstraintModel {
    pub fn new(engine: Arc<GeofenceConstraintEngine>) -> GeofenceConstraintModel {
        GeofenceConstraintModel { engine }
    }
}

impl ConstraintModel for GeofenceConstraintModel {
    fn valid_frontier(
        &self,
        _edge: &routee_compass_core::model::network::Edge,
        _previous_edge: Option<&routee_compass_core::model::network::Edge>,
        _state: &[routee_compass_core::model::state::StateVariable],
        _state_model: &routee_compass_core::model::state::StateModel,
    ) -> Result<bool, routee_compass_core::model::constraint::ConstraintModelError> {
        todo!()
    }

    fn valid_edge(
        &self,
        _edge: &routee_compass_core::model::network::Edge,
    ) -> Result<bool, routee_compass_core::model::constraint::ConstraintModelError> {
        todo!()
    }
}
