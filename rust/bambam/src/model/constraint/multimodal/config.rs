use super::ConstraintConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultimodalConstraintConfig {
    /// name of the mode associated with this edge list
    pub this_mode: String,
    /// constraints to apply when in this mode
    pub constraints: Vec<ConstraintConfig>,
    /// modes that can be used on this trip
    pub available_modes: Vec<String>,
    /// all route ids available in multimodal search. this ordering will be used
    /// to generate an enumeration used in state modeling.
    pub route_ids_input_file: Option<String>,
    /// maximum number of legs allowed in a trip
    pub max_trip_legs: u64,
}
