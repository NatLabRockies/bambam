use bambam_core::model::state::{fieldname, variable::EMPTY_CATEGORICAL_VALUE, CategoricalMapping};
use chrono::{NaiveDateTime, TimeDelta};
use routee_compass_core::model::{
    state::{StateModel, StateModelError, StateVariable},
    traversal::TraversalModelError,
};
use uom::si::f64::Time;

use crate::{model::feature, util::zone::ZoneId};

pub fn create_current_datetime(
    start_time: &NaiveDateTime,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<NaiveDateTime, TraversalModelError> {
    let time: Time = state_model.get_time(state, fieldname::TRIP_TIME)?;
    let time_i64 = time.get::<uom::si::time::second>() as i64;
    start_time
        .checked_add_signed(TimeDelta::seconds(time_i64))
        .ok_or_else(|| {
            TraversalModelError::TraversalModelFailure(format!(
                "overflow when adding {time_i64} seconds to {}",
                start_time
            ))
        })
}

/// quickly confirm if the state vector's leg src zone id is unset/empty.
pub fn src_zone_id_set(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<bool, TraversalModelError> {
    let label = state_model.get_custom_i64(state, feature::fieldname::LEG_SRC_ZONE_ID)?;
    Ok(label != EMPTY_CATEGORICAL_VALUE)
}

/// inspect the trip state for a source zone id, and if found, return it, otherwise None.
pub fn get_src_zone_id<'a>(
    state: &[StateVariable],
    state_model: &StateModel,
    mapping: &'a CategoricalMapping<ZoneId, i64>,
) -> Result<Option<&'a ZoneId>, StateModelError> {
    let label = state_model.get_custom_i64(state, feature::fieldname::LEG_SRC_ZONE_ID)?;
    if label == EMPTY_CATEGORICAL_VALUE {
        return Ok(None);
    }
    let zone_id = mapping.get_categorical(label)?.ok_or_else(|| {
        log::debug!(
            "label {label} has no corresponding ZoneId in mapping: {:?}",
            mapping.get_categories()
        );
        StateModelError::RuntimeError(format!(
            "label {label} has no corresponding ZoneId in mapping"
        ))
    })?;
    Ok(Some(zone_id))
}

/// helper function to write the categorical value representing the provided [ZoneId] to
/// the state vector, after first translating the [ZoneId] to a i64 value via the mapping.
pub fn set_src_zone_id(
    zone_id: &ZoneId,
    state: &mut [StateVariable],
    state_model: &StateModel,
    mapping: &CategoricalMapping<ZoneId, i64>,
) -> Result<(), TraversalModelError> {
    let label = mapping.get_label(zone_id).ok_or_else(|| {
        let msg = format!("zone id '{zone_id}' not present in categorical mapping");
        TraversalModelError::InternalError(msg)
    })?;
    state_model.set_custom_i64(state, feature::fieldname::LEG_SRC_ZONE_ID, label)?;
    Ok(())
}

/// helper function to write the boolean "is_valid" to the state vector.
pub fn set_is_valid(
    is_valid: bool,
    state: &mut [StateVariable],
    state_model: &StateModel,
) -> Result<(), TraversalModelError> {
    state_model.set_custom_bool(
        state,
        feature::fieldname::EDGE_IS_GTFS_FLEX_DESTINATION,
        &is_valid,
    )?;
    Ok(())
}
