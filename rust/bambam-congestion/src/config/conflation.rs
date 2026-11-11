use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters controlling Open Network <-> Reference Network spatial conflation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConflationConfig {
    /// Corridor buffer distance in meters around each open-network edge (defaults to `5.0`).
    #[serde(default = "default_buffer_distance_meters")]
    pub buffer_distance_meters: f64,
    /// Maximum circular mean heading difference in degrees (`[0, 180]`, defaults to `45.0`).
    #[serde(default = "default_max_heading_diff_degrees")]
    pub max_heading_diff_degrees: f64,
    /// Minimum overlapping corridor length in meters required to accept a reference network match (defaults to `1.0`).
    #[serde(default = "default_min_overlap_meters")]
    pub min_overlap_meters: f64,
    /// Whether to cap observed reference speeds above `posted_speed + 10 mph` (or `max_class_speed + 5 mph`).
    #[serde(default = "default_true")]
    pub cap_speed_outliers: bool,
    /// Maximum allowed mph above a link's posted speed limit (defaults to `10.0`).
    #[serde(default = "default_cap_above_posted_mph")]
    pub speed_cap_above_posted_mph: f64,
    /// Maximum allowed mph above a highway class's maximum speed limit when link speed limit is missing (defaults to `5.0`).
    #[serde(default = "default_cap_above_class_max_mph")]
    pub speed_cap_above_class_max_mph: f64,
}

impl Default for ConflationConfig {
    fn default() -> Self {
        Self {
            buffer_distance_meters: default_buffer_distance_meters(),
            max_heading_diff_degrees: default_max_heading_diff_degrees(),
            min_overlap_meters: default_min_overlap_meters(),
            cap_speed_outliers: true,
            speed_cap_above_posted_mph: default_cap_above_posted_mph(),
            speed_cap_above_class_max_mph: default_cap_above_class_max_mph(),
        }
    }
}

fn default_buffer_distance_meters() -> f64 {
    5.0
}

fn default_max_heading_diff_degrees() -> f64 {
    45.0
}

fn default_min_overlap_meters() -> f64 {
    1.0
}

fn default_cap_above_posted_mph() -> f64 {
    10.0
}

fn default_cap_above_class_max_mph() -> f64 {
    5.0
}

fn default_true() -> bool {
    true
}
