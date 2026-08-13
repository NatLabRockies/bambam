use chrono::{DateTime, TimeDelta, Utc};
use routee_compass_core::model::{
    state::{StateModel, StateVariable},
    traversal::default::fieldname,
};
use uom::si::f64::Time;

/// builds a globally-unique identifier for a zone. based on the fact that
/// system_ids are defined as globally unique. as documented at
/// <https://github.com/MobilityData/gbfs/blob/master/gbfs.md#system_informationjson>:
///
/// > [system_id] is a globally unique identifier for the vehicle share system. Each distinct system
/// > or geographic area in which vehicles are operated MUST have its own unique system_id. It
/// > is up to the publisher of the feed to guarantee uniqueness and MUST be checked against
/// > existing system_id fields in systems.csv to ensure this. This value is intended to remain
/// > the same over the life of the system.
/// >
/// > System IDs SHOULD be recognizable as belonging to a particular system as opposed to random
/// > strings - for example, bcycle_austin or biketown_pdx.
pub fn fully_qualified_zone_id(system_id: &str, zone_feature_index: usize) -> String {
    format!("{system_id}#{zone_feature_index}")
}

/// helper function to calculate the current datetime based on the start time and trip_time values.
pub fn current_datetime(
    start_time: DateTime<Utc>,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<DateTime<Utc>, String> {
    let time: Time = state_model
        .get_time(state, fieldname::TRIP_TIME)
        .map_err(|e| {
            format!(
                "failure getting '{}' from state vector: {e}",
                fieldname::TRIP_TIME
            )
        })?;
    let time_i64 = time.get::<uom::si::time::second>() as i64;
    start_time
        .checked_add_signed(TimeDelta::seconds(time_i64))
        .ok_or_else(|| {
            let msg = format!(
                "adding {} seconds to {} was out of bounds",
                time_i64,
                start_time.to_rfc3339()
            );
            msg
        })
}
