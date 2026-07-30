use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GbfsConstraintParams {
    /// time the trip starts
    pub start_time: DateTime<Utc>,
}
