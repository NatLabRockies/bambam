use bambam_core::util::date_deserialization_ops::deserialize_naive_datetime;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::model::traversal::transit::config::WalkSpeed;

#[derive(Serialize, Deserialize)]
pub struct TransitTraversalQuery {
    #[serde(deserialize_with = "deserialize_naive_datetime")]
    pub start_datetime: NaiveDateTime, // Fix deserialization
    /// If true, we maintain a DWELL_TIME state variable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_dwell_time: Option<bool>,
    /// Optional per-query walking speed `(value, unit)` override used to compute vertex-to-stop boarding delay
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walk_speed: Option<WalkSpeed>,
}
