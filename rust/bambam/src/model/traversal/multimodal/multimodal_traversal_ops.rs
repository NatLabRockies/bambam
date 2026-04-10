use std::num::NonZeroU64;
use std::sync::OnceLock;

use bambam_core::model::bambam_state;
use bambam_core::model::state::{
    fieldname, multimodal_state_ops as state_ops, LegIdx, MultimodalStateMapping,
};
use routee_compass_core::model::state::{StateModel, StateModelError, StateVariable};
use serde_json::json;
use uom::si::f64::{Length, Time};

/// tests the active travel mode. if it does not match the mode of this edge,
/// then perform a mode switch, creating a new trip leg and assigning the mode label.
pub fn mode_switch(
    state: &mut [StateVariable],
    state_model: &StateModel,
    prev_mode: &str,
    mode_to_state: &MultimodalStateMapping,
    max_trip_legs: NonZeroU64,
) -> Result<(), StateModelError> {
    // grab the leg_idx and leg mode if it exists. allow None cases to flow through
    // and handle error cases.
    let leg_idx_opt = state_ops::get_active_leg_idx(state, state_model)?;
    let leg_and_mode_opt = match leg_idx_opt {
        Some(leg_idx) => {
            let mode = state_ops::get_existing_leg_mode(
                state,
                leg_idx,
                state_model,
                max_trip_legs,
                mode_to_state,
            )?;
            Some((leg_idx, mode))
        }
        None => None,
    };

    match leg_and_mode_opt {
        Some((_, leg_mode)) if leg_mode == prev_mode => {
            // leg exists but no change in mode -> return early
        }
        _ => {
            // no leg assigned or a change in mode -> add the new leg
            let next_leg_idx =
                state_ops::increment_active_leg_idx(state, state_model, max_trip_legs)?;
            state_ops::set_leg_mode(state, next_leg_idx, prev_mode, state_model, mode_to_state)?;
        }
    };
    Ok(())
}

/// copies edge_distance and edge_time into the mode and leg accumulators used by
/// multimodal routing.
pub fn update_accumulators(
    state: &mut [StateVariable],
    state_model: &StateModel,
    mode: &str,
    leg_idx: LegIdx,
    mode_to_state: &MultimodalStateMapping,
    max_trip_legs: NonZeroU64,
) -> Result<(), StateModelError> {
    let distance: Length = state_model.get_distance(state, fieldname::EDGE_DISTANCE)?;
    let time: Time = state_model.get_time(state, fieldname::EDGE_TIME)?;
    let mode = state_ops::get_existing_leg_mode(
        state,
        leg_idx,
        state_model,
        max_trip_legs,
        mode_to_state,
    )?;

    let d_leg = fieldname::leg_distance_fieldname(leg_idx);
    let t_leg = fieldname::leg_time_fieldname(leg_idx);
    let d_mode = fieldname::mode_distance_fieldname(mode);
    let t_mode = fieldname::mode_time_fieldname(mode);
    state_model.add_distance(state, &d_leg, &distance)?;
    state_model.add_time(state, &t_leg, &time)?;
    state_model.add_distance(state, &d_mode, &distance)?;
    state_model.add_time(state, &t_mode, &time)?;
    Ok(())
}

/// this hack is used because StateModel::contains_key expects a &String, which would require
/// a new string allocation each time it is invoked here. to avoid this, we store a static [OnceLock]'d
/// String and reference it in the call once initialized. this should be removed once things are updated
/// on routee-compass-core, see <https://github.com/NatLabRockies/routee-compass/pull/493>.
static ROUTE_ID_STRING: OnceLock<String> = OnceLock::new();

/// tests if route_id is set, and if so, copies it to the current trip leg.
pub fn update_route_id(
    state: &mut [StateVariable],
    state_model: &StateModel,
    mode: &str,
    leg_idx: LegIdx,
    max_trip_legs: NonZeroU64,
) -> Result<(), StateModelError> {
    let route_id_key = ROUTE_ID_STRING.get_or_init(|| bambam_state::ROUTE_ID.to_string());
    if state_model.contains_key(route_id_key) {
        let route_id_label = state_model.get_custom_i64(state, bambam_state::ROUTE_ID)?;
        state_ops::set_leg_route_id_raw(state, leg_idx, route_id_label, state_model)
    } else {
        Ok(())
    }
}

/// helper function for applying the label/categorical mapping in the
/// context of serializing a value on an output multimodal search state JSON.
pub fn apply_mapping_for_serialization(
    state_json: &mut serde_json::Value,
    name: &str,
    leg_idx: LegIdx,
    mapping: &MultimodalStateMapping,
) -> Result<(), StateModelError> {
    if let Some(v) = state_json.get_mut(name) {
        let label = v.as_i64().ok_or_else(|| {
            StateModelError::RuntimeError(format!(
                "unable to get label (i64) value for leg index, key {leg_idx}, {name}"
            ))
        })?;
        if label < 0 {
            *v = json![""]; // no mode assigned
        } else {
            let cat = mapping.get_categorical(label)?.ok_or_else(|| {
                StateModelError::RuntimeError(format!(
                    "while serializing multimodal state, mapping failed for name, leg index, label: {name}, {leg_idx}, {label}"
                ))
            })?;
            *v = json![cat.to_string()];
        }
    }
    Ok(())
}
