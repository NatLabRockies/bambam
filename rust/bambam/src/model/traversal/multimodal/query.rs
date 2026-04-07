//! values deserialized from a search query which can be used to override defaults.
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultimodalTraversalQuery {
    /// allows, at query time, for users to modify the list of available modes for a search.
    /// if not provided, the [`super::MultimodalTraversalConfig`] value will be used.
    pub available_modes: Option<Vec<String>>,

    /// allows, at query time, for users to modify the list of available route ids for a search.
    /// if not provided, the [`super::MultimodalTraversalConfig`] value will be used.
    pub available_route_ids: Option<Vec<String>>,

    /// each mode transition results in a new trip leg. this value restricts
    /// the number of allowed mode transitions. this is both a domain-specific
    /// configuration value to limit to realistic mode usage and also an algorithmic
    /// configuration value as space complexity grows k^n for k modes, n legs.
    ///
    /// default value: 1 trip leg (unimodal trip).
    #[serde(default = "unimodal_trip")]
    pub max_trip_legs: NonZeroU64,
}

/// use 1 trip leg by default.
pub fn unimodal_trip() -> NonZeroU64 {
    NonZeroU64::MIN
}
