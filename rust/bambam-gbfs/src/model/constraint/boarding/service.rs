use std::sync::Arc;

use super::{BoardingConstraintEngine, BoardingConstraintModel};

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError, ConstraintModelService},
    state::StateModel,
};

pub struct BoardingConstraintService {
    pub engine: Arc<BoardingConstraintEngine>,
}

impl BoardingConstraintService {
    pub fn new(engine: BoardingConstraintEngine) -> BoardingConstraintService {
        BoardingConstraintService {
            engine: Arc::new(engine),
        }
    }
}

impl ConstraintModelService for BoardingConstraintService {
    fn build(
        &self,
        _query: &serde_json::Value,
        _state_model: Arc<StateModel>,
    ) -> Result<Arc<dyn ConstraintModel>, ConstraintModelError> {
        Ok(Arc::new(BoardingConstraintModel::new(self.engine.clone())))
    }
}
