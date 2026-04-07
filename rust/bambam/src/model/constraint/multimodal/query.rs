use std::{num::NonZeroU64, sync::Once};

use routee_compass_core::model::constraint::ConstraintModelError;
use serde::{Deserialize, Serialize};

use crate::model::constraint::multimodal::{Constraint, ConstraintConfig};

/// query-time arguments to the multimodal constraint model
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MultimodalConstraintModelQuery {
    /// constraints to apply when in this mode. without constraints, the multimodal search
    /// space quickly grows intractable and will produce unrealistic behaviors.
    pub constraints: Option<Vec<ConstraintConfig>>,
    /// each mode transition results in a new trip leg. this value restricts
    /// the number of allowed mode transitions. this is both a domain-specific
    /// configuration value to limit to realistic mode usage and also an algorithmic
    /// configuration value as space complexity grows k^n for k modes, n legs.
    pub max_trip_legs: NonZeroU64,
}

/// tracks whether to log (once) the warning about empty constraints on queries.
static EMPTY_CONSTRAINTS_WARNING: Once = Once::new();

impl MultimodalConstraintModelQuery {
    pub fn build_constraints(&self) -> Result<Vec<Constraint>, ConstraintModelError> {
        let constraints = self
            .constraints
            .as_ref()
            .unwrap_or(&vec![])
            .iter()
            .map(Constraint::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        if constraints.is_empty() {
            EMPTY_CONSTRAINTS_WARNING.call_once(|| {
                log::warn!("encountered a query with no multimodal constraints! in multimodal graphs this can lead to intractable search areas.");
            });
        }
        Ok(constraints)
    }
}
