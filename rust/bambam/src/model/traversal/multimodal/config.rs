use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MultimodalTraversalConfig {
    /// mode associated with this edge list
    pub this_mode: String,
    /// all modes available in multimdal search. this ordering will be used
    /// to generate an enumeration used in state modeling.
    pub available_modes: Vec<String>,
}
