use std::num::NonZeroU64;

use crate::model::state::{LegIdx, CategoricalStateMapping};
use routee_compass_core::model::state::{StateModel, StateModelError, StateVariable};
use serde_json::json;
use uom::si::f64::{Energy, Length, Time};

use super::fieldname;

/// inspect the current active leg for a trip
pub fn get_active_leg_idx(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Option<LegIdx>, StateModelError> {
    let leg_i64 = state_model.get_custom_i64(state, fieldname::ACTIVE_LEG)?;
    if leg_i64 < 0 {
        Ok(None)
    } else {
        let leg_u64 = leg_i64.try_into().map_err(|_e| {
            StateModelError::RuntimeError(format!(
                "internal error: while getting active trip leg, unable to parse {leg_i64} as a u64"
            ))
        })?;
        Ok(Some(leg_u64))
    }
}

/// inspect the current active leg mode for a trip. if the trip
/// has no leg, returns None.
pub fn get_active_leg_mode<'a>(
    state: &[StateVariable],
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
    mode_to_state: &'a CategoricalStateMapping,
) -> Result<Option<&'a str>, StateModelError> {
    match get_active_leg_idx(state, state_model)? {
        None => Ok(None),
        Some(leg_idx) => {
            let mode =
                get_existing_leg_mode(state, leg_idx, state_model, max_trip_legs, mode_to_state)?;
            Ok(Some(mode))
        }
    }
}

/// use the active leg index to count the number of trip legs in this state vector
pub fn get_n_legs(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<usize, StateModelError> {
    match get_active_leg_idx(state, state_model)? {
        None => Ok(0),
        Some(leg_idx) => {
            let count: usize = (leg_idx + 1).try_into().map_err(|_e| {
                StateModelError::RuntimeError(format!(
                    "internal error: unable to convert leg index {leg_idx} from u64 into usize"
                ))
            })?;
            Ok(count)
        }
    }
}

/// a constraint model action, this tests if the addition of a single edge mode breaks the limit on trip legs
/// set during configuration. this is determined based on a reading of the constraint model.
pub fn appending_edge_mode_is_valid(
    state: &[StateVariable],
    state_model: &StateModel,
    leg_mode: &str,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalStateMapping,
) -> Result<bool, StateModelError> {
    // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
    let active_mode = get_active_leg_mode(state, state_model, max_trip_legs, mode_to_state)
        .map_err(|e| {
            StateModelError::RuntimeError(format!("while validating trip leg count, {e}"))
        })?;
    let n_existing_legs = get_n_legs(state, state_model).map_err(|e| {
        StateModelError::RuntimeError(format!(
            "while getting number of trip legs for this trip: {e}"
        ))
    })?;
    let n_legs = match active_mode {
        Some(active_mode) if active_mode != leg_mode => n_existing_legs + 1,
        _ => n_existing_legs,
    };

    let max_legs_usize = max_trip_legs.get() as usize;
    Ok(n_legs <= max_legs_usize)
}

/// report if any trip data has been recorded for the given trip leg.
/// this uses the fact that any trip leg must have a leg mode, and leg modes
/// are stored with non-negative integer values, negative denotes "empty".
/// see [`super::state_variable`] for the leg mode variable configuration.
pub fn contains_leg(
    state: &[StateVariable],
    leg_idx: LegIdx,
    state_model: &StateModel,
) -> Result<bool, StateModelError> {
    let name = fieldname::leg_mode_fieldname(leg_idx);
    let label = state_model.get_custom_i64(state, &name)?;
    Ok(label >= 0)
}

/// get the travel mode for a leg.
pub fn get_leg_mode_label(
    state: &[StateVariable],
    leg_idx: LegIdx,
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
) -> Result<Option<i64>, StateModelError> {
    validate_leg_idx(leg_idx, max_trip_legs)?;
    let name = fieldname::leg_mode_fieldname(leg_idx);
    let label = state_model.get_custom_i64(state, &name)?;
    if label < 0 {
        Ok(None)
    } else {
        Ok(Some(label))
    }
}

/// get the travel mode for a leg. assumed that the leg mode exists,
/// if the mode is not set, it is an error.
pub fn get_existing_leg_mode<'a>(
    state: &[StateVariable],
    leg_idx: LegIdx,
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
    mode_to_state: &'a CategoricalStateMapping,
) -> Result<&'a str, StateModelError> {
    let label_opt = get_leg_mode_label(state, leg_idx, state_model, max_trip_legs)?;
    match label_opt {
        None => Err(StateModelError::RuntimeError(format!(
            "Internal Error: get_leg_mode called on leg idx {leg_idx} but mode label is not set"
        ))),
        Some(label) => mode_to_state
            .get_categorical(label)?
            .ok_or_else(|| {
                StateModelError::RuntimeError(format!(
                    "internal error, leg {leg_idx} has invalid mode label {label}"
                ))
            })
            .map(|s| s.as_str()),
    }
}

pub fn get_leg_distance(
    state: &[StateVariable],
    leg_idx: LegIdx,
    state_model: &StateModel,
) -> Result<Length, StateModelError> {
    let name = fieldname::leg_distance_fieldname(leg_idx);
    state_model.get_distance(state, &name)
}

pub fn get_leg_time(
    state: &[StateVariable],
    leg_idx: LegIdx,
    state_model: &StateModel,
) -> Result<Time, StateModelError> {
    let name = fieldname::leg_time_fieldname(leg_idx);
    state_model.get_time(state, &name)
}

pub fn get_leg_energy(
    state: &[StateVariable],
    leg_idx: LegIdx,
    state_model: &StateModel,
) -> Result<Energy, StateModelError> {
    let name = fieldname::leg_energy_fieldname(leg_idx);
    state_model.get_energy(state, &name)
}

pub fn get_leg_route_id<'a>(
    state: &[StateVariable],
    leg_idx: LegIdx,
    state_model: &StateModel,
    route_id_mapping: &'a CategoricalStateMapping,
) -> Result<Option<&'a String>, StateModelError> {
    let name = fieldname::leg_route_id_fieldname(leg_idx);
    let route_id_label = state_model.get_custom_i64(state, &name)?;
    let route_id = route_id_mapping.get_categorical(route_id_label)?;
    Ok(route_id)
}

pub fn get_mode_distance(
    state: &[StateVariable],
    mode: &str,
    state_model: &StateModel,
) -> Result<Length, StateModelError> {
    let name = fieldname::mode_distance_fieldname(mode);
    state_model.get_distance(state, &name)
}

pub fn get_mode_time(
    state: &[StateVariable],
    mode: &str,
    state_model: &StateModel,
) -> Result<Time, StateModelError> {
    let name = fieldname::mode_time_fieldname(mode);
    state_model.get_time(state, &name)
}

/// retrieves the sequence of mode labels stored on this state. stops when an unset
/// mode label is encountered.
pub fn get_mode_label_sequence(
    state: &[StateVariable],
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
) -> Result<Vec<i64>, StateModelError> {
    let mut labels: Vec<i64> = vec![];

    for leg_idx in 0..max_trip_legs.get() {
        let mode_label_opt = get_leg_mode_label(state, leg_idx, state_model, max_trip_legs)?;
        match mode_label_opt {
            None => break,
            Some(mode_label) => {
                labels.push(mode_label);
            }
        }
    }

    Ok(labels)
}

/// retrieves the sequence of modes stored on this state. stops when an unset
/// mode label is encountered.
pub fn get_mode_sequence(
    state: &[StateVariable],
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalStateMapping,
) -> Result<Vec<String>, StateModelError> {
    let mut modes: Vec<String> = vec![];
    let mut leg_idx = 0;
    while contains_leg(state, leg_idx, state_model)? {
        let mode =
            get_existing_leg_mode(state, leg_idx, state_model, max_trip_legs, mode_to_state)?;
        modes.push(mode.to_string());
        leg_idx += 1;
    }
    Ok(modes)
}

/// increments the value at [`fieldname::ACTIVE_LEG`].
/// when ACTIVE_LEG is negative (no active leg), it becomes zero.
/// when it is a number in [0, max_legs-1), it is incremented by one.
/// returns the new index value.
pub fn increment_active_leg_idx(
    state: &mut [StateVariable],
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
) -> Result<LegIdx, StateModelError> {
    // get the index of the next leg
    let next_leg_idx_u64 = match get_active_leg_idx(state, state_model)? {
        Some(leg_idx) => {
            let next = leg_idx + 1;
            validate_leg_idx(next, max_trip_legs)?;
            next
        }
        None => 0,
    };
    // as an i64, to match the storage format
    let next_leg_idx: i64 = next_leg_idx_u64.try_into().map_err(|_e| {
        StateModelError::RuntimeError(format!(
            "internal error: while getting active trip leg, unable to parse {next_leg_idx_u64} as a i64"
        ))
    })?;

    // increment the value in the state vector
    state_model.set_custom_i64(state, fieldname::ACTIVE_LEG, &next_leg_idx)?;
    Ok(next_leg_idx_u64)
}

/// sets the mode value for the given leg. performs mapping from Mode -> i64 which is
/// the storage type for Mode in the state vector.
pub fn set_leg_mode(
    state: &mut [StateVariable],
    leg_idx: LegIdx,
    mode: &str,
    state_model: &StateModel,
    mode_to_state: &CategoricalStateMapping,
) -> Result<(), StateModelError> {
    let mode_label = mode_to_state.get_label(mode).ok_or_else(|| {
        StateModelError::RuntimeError(format!("mode mapping has no entry for '{mode}' mode"))
    })?;
    let name = fieldname::leg_mode_fieldname(leg_idx);
    state_model.set_custom_i64(state, &name, mode_label)
}

/// sets the mode value for the given leg. performs mapping from Mode -> i64 which is
/// the storage type for Mode in the state vector.
pub fn set_leg_route_id(
    state: &mut [StateVariable],
    leg_idx: LegIdx,
    route_id: &str,
    state_model: &StateModel,
    route_id_to_state: &CategoricalStateMapping,
) -> Result<(), StateModelError> {
    let route_id_label = route_id_to_state.get_label(route_id).ok_or_else(|| {
        StateModelError::RuntimeError(format!(
            "route_id mapping has no entry for '{route_id}' route id"
        ))
    })?;
    let name = fieldname::leg_route_id_fieldname(leg_idx);
    state_model.set_custom_i64(state, &name, route_id_label)
}

/// sets the route id value for the given leg. executed without first testing that
/// the label maps to a value in a route id enumeration.
pub fn set_leg_route_id_raw(
    state: &mut [StateVariable],
    leg_idx: LegIdx,
    route_id_label: i64,
    state_model: &StateModel,
) -> Result<(), StateModelError> {
    let name = fieldname::leg_route_id_fieldname(leg_idx);
    state_model.set_custom_i64(state, &name, &route_id_label)
}

/// validates leg_idx values, which must be in range [0, max_trip_legs)
pub fn validate_leg_idx(leg_idx: LegIdx, max_trip_legs: NonZeroU64) -> Result<(), StateModelError> {
    if leg_idx >= max_trip_legs.get() {
        Err(StateModelError::RuntimeError(format!(
            "invalid leg id {leg_idx} >= max leg id {max_trip_legs}"
        )))
    } else {
        Ok(())
    }
}

/// helper function for creating a descriptive error when attempting to apply
/// the multimodal traversal model on a state that has not activated it's first trip leg.
pub fn error_inactive_state_traversal(
    state: &[StateVariable],
    state_model: &StateModel,
) -> StateModelError {
    let next_json = state_model.serialize_state(state, false).unwrap_or_else(
        |e| json!({"message": "unable to serialize state!", "error": format!("{e}")}),
    );
    let next_string = serde_json::to_string_pretty(&next_json)
        .unwrap_or_else(|_e| String::from("<unable to serialize state!>"));
    StateModelError::RuntimeError(format!(
        "attempting multimodal traversal with state that has no active leg: {next_string}"
    ))
}
