use std::sync::Arc;

use routee_compass_core::model::constraint::ConstraintModel;

use super::BoardingConstraintEngine;

pub struct BoardingConstraintModel {
    pub engine: Arc<BoardingConstraintEngine>,
}

/// restricts where GBFS boarding can occur by zone
impl BoardingConstraintModel {
    pub fn new(engine: Arc<BoardingConstraintEngine>) -> BoardingConstraintModel {
        BoardingConstraintModel { engine }
    }
}

impl ConstraintModel for BoardingConstraintModel {
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
