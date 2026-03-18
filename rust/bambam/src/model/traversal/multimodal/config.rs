use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MultimodalTraversalConfig {
    /// mode associated with this edge list
    pub this_mode: String,
    /// all modes available in multimdal search. this ordering will be used
    /// to generate an enumeration used in state modeling.
    pub available_modes: Vec<String>,
    /// all route ids available in multimodal search. this ordering will be used
    /// to generate an enumeration used in state modeling.
    pub route_ids_input_file: Option<String>,
    /// for a given search, the limit to the number of mode transitions that can occur
    pub max_trip_legs: NonZeroU64,
}
