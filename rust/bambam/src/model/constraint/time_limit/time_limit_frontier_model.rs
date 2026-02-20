use crate::model::constraint::time_limit::TimeLimitConfig;
use bambam_core::model::bambam_state;
use routee_compass_core::{
    algorithm::search::Direction,
    model::{
        constraint::{ConstraintModel, ConstraintModelError},
        network::{Edge, VertexId},
        state::{StateModel, StateVariable},
        unit::TimeUnit,
    },
};
use std::{borrow::Cow, collections::HashMap};
use uom::si::f64::Time;

pub struct TimeLimitConstraintModel {
    pub time_limit: Time,
}

impl ConstraintModel for TimeLimitConstraintModel {
    fn valid_frontier(
        &self,
        _edge: &Edge,
        _previous_edge: Option<&Edge>,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        let time = state_model
            .get_time(state, bambam_state::TRIP_TIME)
            .map_err(|e| ConstraintModelError::BuildError(e.to_string()))?;
        let is_valid = time <= self.time_limit;
        Ok(is_valid)
    }

    fn valid_edge(&self, _edge: &Edge) -> Result<bool, ConstraintModelError> {
        Ok(true)
    }
}
