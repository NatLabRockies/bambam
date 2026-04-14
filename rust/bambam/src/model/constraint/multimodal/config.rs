use std::num::NonZeroU64;

use super::ConstraintConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MultimodalConstraintConfig {
    /// name of the mode associated with this edge list
    pub this_mode: String,
    /// modes that can be used on this trip
    pub available_modes: Vec<String>,
}
