use bambam_core::util::date_deserialization_ops::deserialize_naive_datetime;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uom::si::f64::Time;

#[derive(Serialize, Deserialize)]
pub struct TransitTraversalQuery {
    #[serde(deserialize_with = "deserialize_naive_datetime")]
    pub start_datetime: NaiveDateTime, // Fix deserialization
    /// If true, we maintain a DWELL_TIME state variable
    pub record_dwell_time: Option<bool>,
}
