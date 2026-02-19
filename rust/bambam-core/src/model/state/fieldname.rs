//! field names for state variables related to multimodal routing. accumulators
//! for trip legs and mode-specific metrics need to be stored in fields with
//! normalized fieldname structures that are programmatically generated.

use crate::model::state::LegIdx;
pub use routee_compass_core::model::traversal::default::fieldname::*;

/// the id of the active leg. zero if no leg is active. 1+ are leg identifiers.
pub const ACTIVE_LEG: &str = "active_leg";

/// the state variable name containing the mode for a given leg id
pub fn leg_mode_fieldname(leg_idx: LegIdx) -> String {
    leg_fieldname(leg_idx, "mode")
}

/// the state variable name containing the distance for a given leg id
pub fn leg_distance_fieldname(leg_idx: LegIdx) -> String {
    leg_fieldname(leg_idx, "distance")
}

/// the state variable name containing the time for a given leg id
pub fn leg_time_fieldname(leg_idx: LegIdx) -> String {
    leg_fieldname(leg_idx, "time")
}

/// the state variable name containing the route id for a given leg id
pub fn leg_route_id_fieldname(leg_idx: LegIdx) -> String {
    leg_fieldname(leg_idx, "route_id")
}

/// the state variable name containing the distance for a given mode
pub fn mode_distance_fieldname(mode: &str) -> String {
    mode_fieldname(mode, "distance")
}

/// the state variable name containing the time for a given mode
pub fn mode_time_fieldname(mode: &str) -> String {
    mode_fieldname(mode, "time")
}

/// helper function for creating normalized and enumerated fieldnames
/// for fields associated with a trip leg.
fn leg_fieldname(leg_idx: LegIdx, field: &str) -> String {
    format!("leg_{leg_idx}_{field}")
}

/// helper function for creating normalized fieldnames for fields
/// accumulating metrics for a given travel mode.
fn mode_fieldname(mode: &str, field: &str) -> String {
    format!("mode_{mode}_{field}")
}
