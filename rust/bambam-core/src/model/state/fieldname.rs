//! field names for state variables related to multimodal routing. accumulators
//! for trip legs and mode-specific metrics need to be stored in fields with
//! normalized fieldname structures that are programmatically generated.

use std::borrow::Cow;

use crate::model::state::LegIdx;
pub use routee_compass_core::model::traversal::default::fieldname::*;

/// the id of the active leg. zero if no leg is active. 1+ are leg identifiers.
pub const ACTIVE_LEG: &str = "active_leg";

const LEG_MODE_FIELDNAMES: [&str; 16] = [
    "leg_0_mode",
    "leg_1_mode",
    "leg_2_mode",
    "leg_3_mode",
    "leg_4_mode",
    "leg_5_mode",
    "leg_6_mode",
    "leg_7_mode",
    "leg_8_mode",
    "leg_9_mode",
    "leg_10_mode",
    "leg_11_mode",
    "leg_12_mode",
    "leg_13_mode",
    "leg_14_mode",
    "leg_15_mode",
];

const LEG_DISTANCE_FIELDNAMES: [&str; 16] = [
    "leg_0_distance",
    "leg_1_distance",
    "leg_2_distance",
    "leg_3_distance",
    "leg_4_distance",
    "leg_5_distance",
    "leg_6_distance",
    "leg_7_distance",
    "leg_8_distance",
    "leg_9_distance",
    "leg_10_distance",
    "leg_11_distance",
    "leg_12_distance",
    "leg_13_distance",
    "leg_14_distance",
    "leg_15_distance",
];

const LEG_TIME_FIELDNAMES: [&str; 16] = [
    "leg_0_time",
    "leg_1_time",
    "leg_2_time",
    "leg_3_time",
    "leg_4_time",
    "leg_5_time",
    "leg_6_time",
    "leg_7_time",
    "leg_8_time",
    "leg_9_time",
    "leg_10_time",
    "leg_11_time",
    "leg_12_time",
    "leg_13_time",
    "leg_14_time",
    "leg_15_time",
];

const LEG_ENERGY_FIELDNAMES: [&str; 16] = [
    "leg_0_energy",
    "leg_1_energy",
    "leg_2_energy",
    "leg_3_energy",
    "leg_4_energy",
    "leg_5_energy",
    "leg_6_energy",
    "leg_7_energy",
    "leg_8_energy",
    "leg_9_energy",
    "leg_10_energy",
    "leg_11_energy",
    "leg_12_energy",
    "leg_13_energy",
    "leg_14_energy",
    "leg_15_energy",
];

const LEG_ROUTE_ID_FIELDNAMES: [&str; 16] = [
    "leg_0_route_id",
    "leg_1_route_id",
    "leg_2_route_id",
    "leg_3_route_id",
    "leg_4_route_id",
    "leg_5_route_id",
    "leg_6_route_id",
    "leg_7_route_id",
    "leg_8_route_id",
    "leg_9_route_id",
    "leg_10_route_id",
    "leg_11_route_id",
    "leg_12_route_id",
    "leg_13_route_id",
    "leg_14_route_id",
    "leg_15_route_id",
];

/// the state variable name containing the mode for a given leg id
#[inline]
pub fn leg_mode_fieldname(leg_idx: LegIdx) -> Cow<'static, str> {
    match LEG_MODE_FIELDNAMES.get(leg_idx as usize) {
        Some(&s) => Cow::Borrowed(s),
        None => Cow::Owned(leg_fieldname(leg_idx, "mode")),
    }
}

/// the state variable name containing the distance for a given leg id
#[inline]
pub fn leg_distance_fieldname(leg_idx: LegIdx) -> Cow<'static, str> {
    match LEG_DISTANCE_FIELDNAMES.get(leg_idx as usize) {
        Some(&s) => Cow::Borrowed(s),
        None => Cow::Owned(leg_fieldname(leg_idx, "distance")),
    }
}

/// the state variable name containing the time for a given leg id
#[inline]
pub fn leg_time_fieldname(leg_idx: LegIdx) -> Cow<'static, str> {
    match LEG_TIME_FIELDNAMES.get(leg_idx as usize) {
        Some(&s) => Cow::Borrowed(s),
        None => Cow::Owned(leg_fieldname(leg_idx, "time")),
    }
}

/// the state variable name containing the energy for a given leg id
#[inline]
pub fn leg_energy_fieldname(leg_idx: LegIdx) -> Cow<'static, str> {
    match LEG_ENERGY_FIELDNAMES.get(leg_idx as usize) {
        Some(&s) => Cow::Borrowed(s),
        None => Cow::Owned(leg_fieldname(leg_idx, "energy")),
    }
}

/// the state variable name containing the route id for a given leg id
#[inline]
pub fn leg_route_id_fieldname(leg_idx: LegIdx) -> Cow<'static, str> {
    match LEG_ROUTE_ID_FIELDNAMES.get(leg_idx as usize) {
        Some(&s) => Cow::Borrowed(s),
        None => Cow::Owned(leg_fieldname(leg_idx, "route_id")),
    }
}

/// the state variable name containing the distance for a given mode
#[inline]
pub fn mode_distance_fieldname(mode: &str) -> Cow<'static, str> {
    match mode {
        "walk" => Cow::Borrowed("mode_walk_distance"),
        "bike" => Cow::Borrowed("mode_bike_distance"),
        "drive" => Cow::Borrowed("mode_drive_distance"),
        "transit" => Cow::Borrowed("mode_transit_distance"),
        "tnc" => Cow::Borrowed("mode_tnc_distance"),
        "gtfs-flex" => Cow::Borrowed("mode_gtfs-flex_distance"),
        "gbfs" => Cow::Borrowed("mode_gbfs_distance"),
        _ => Cow::Owned(mode_fieldname(mode, "distance")),
    }
}

/// the state variable name containing the time for a given mode
#[inline]
pub fn mode_time_fieldname(mode: &str) -> Cow<'static, str> {
    match mode {
        "walk" => Cow::Borrowed("mode_walk_time"),
        "bike" => Cow::Borrowed("mode_bike_time"),
        "drive" => Cow::Borrowed("mode_drive_time"),
        "transit" => Cow::Borrowed("mode_transit_time"),
        "tnc" => Cow::Borrowed("mode_tnc_time"),
        "gtfs-flex" => Cow::Borrowed("mode_gtfs-flex_time"),
        "gbfs" => Cow::Borrowed("mode_gbfs_time"),
        _ => Cow::Owned(mode_fieldname(mode, "time")),
    }
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
