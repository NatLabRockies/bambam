use std::collections::HashMap;

use crate::model::state::{
    multimodal_state_ops as state_ops, MultimodalMapping, MultimodalStateMapping,
};
use itertools::Itertools;
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Edge,
    state::{StateModel, StateVariable},
};
use uom::si::f64::Time;

/// count how many times a travel mode is used during a trip by each trip leg.
pub fn get_mode_counts(
    state: &[StateVariable],
    state_model: &StateModel,
    max_trip_legs: u64,
    mode_to_state: &MultimodalStateMapping,
) -> Result<HashMap<String, usize>, ConstraintModelError> {
    let modes = state_ops::get_mode_sequence(state, state_model, max_trip_legs, mode_to_state)
        .map_err(|e| {
            ConstraintModelError::ConstraintModelError(
                (format!("while getting mode counts for this trip: {e}")),
            )
        })?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for mode in modes.into_iter() {
        counts.entry(mode).and_modify(|cnt| *cnt += 1).or_insert(1);
    }
    Ok(counts)
}

/// validates the observed number of mode counts against the provided limits
pub fn valid_mode_counts(counts: &HashMap<String, usize>, limits: &HashMap<String, usize>) -> bool {
    for (mode, observed) in counts.iter() {
        match limits.get(mode) {
            Some(limit) if observed > limit => return false,
            None => return false,
            _ => { /* no op */ }
        }
    }
    true
}

pub fn valid_mode_time(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, Time>,
) -> Result<bool, ConstraintModelError> {
    for (mode, limit) in limits.iter() {
        let mode_time = state_ops::get_mode_time(state, mode, state_model).map_err(|e| {
            ConstraintModelError::ConstraintModelError(
                (format!("while validating mode time limits for '{mode}': {e}")),
            )
        })?;
        if &mode_time > limit {
            return Ok(false);
        }
    }
    Ok(true)
}
