use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GbfsTraversalParams {
    /// datetime we start this trip.
    pub start_time: DateTime<Utc>,
}
