//! values deserialized from a search query which can be used to override defaults.
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::model::constraint::multimodal::ConstraintConfig;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultimodalTraversalQuery {
    /// each mode transition results in a new trip leg. this value restricts
    /// the number of allowed mode transitions. this is both a domain-specific
    /// configuration value to limit to realistic mode usage and also an algorithmic
    /// configuration value as space complexity grows k^n for k modes, n legs.
    ///
    /// if omitted, inferred from `constraints` (`exact_sequences`) or defaults to 1.
    #[serde(default)]
    pub max_trip_legs: Option<NonZeroU64>,
    #[serde(default)]
    pub constraints: Option<Vec<ConstraintConfig>>,
}

/// use 1 trip leg by default.
pub fn unimodal_trip() -> NonZeroU64 {
    NonZeroU64::MIN
}

impl MultimodalTraversalQuery {
    pub fn resolved_max_trip_legs(&self) -> NonZeroU64 {
        if let Some(explicit) = self.max_trip_legs {
            return explicit;
        }
        if let Some(configs) = &self.constraints {
            let max_seq_len = configs
                .iter()
                .filter_map(|c| match c {
                    ConstraintConfig::ExactSequences { values } => {
                        values.iter().map(|s| s.len()).max()
                    }
                    _ => None,
                })
                .max()
                .unwrap_or(1);
            return NonZeroU64::new(max_seq_len as u64).unwrap_or(NonZeroU64::MIN);
        }
        unimodal_trip()
    }
}
