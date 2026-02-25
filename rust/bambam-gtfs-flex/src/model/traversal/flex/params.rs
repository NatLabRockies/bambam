use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GtfsFlexParams {
    /// start time of the trip. used in conjunction with the source zone
    /// to determine the valid destination zones for Service Types 2 + 3.
    pub start_time: NaiveDateTime,
}
