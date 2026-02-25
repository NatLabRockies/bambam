use routee_compass_core::model::state::{StateModel, StateVariable};
use serde::{Deserialize, Serialize};

use crate::model::destination::DestinationError;

/// filter(s) to apply while collecting destinations. these are applied
/// regardless of any binning configuration.
#[derive(Clone, Debug)]
pub struct DestinationFilter(Vec<DestinationPredicate>);

/// additional modifiers to apply when collecting destinations for a bin.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum DestinationPredicateConfig {
    /// only accept destinations where the provided feature, a boolean value,
    /// is true (or, if negate == true, where the feature is false).
    Boolean {
        /// state variable feature to match
        feature: String,
        /// if true, invert the value stored at the feature    
        negate: bool,
    },
}

#[derive(Clone, Debug)]
pub enum DestinationPredicate {
    Boolean { feature: String, negate: bool },
}

impl DestinationFilter {
    pub fn iter(&self) -> std::slice::Iter<'_, DestinationPredicate> {
        self.0.iter()
    }
    pub fn valid_destination(
        &self,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, DestinationError> {
        for pred in self.iter() {
            if !pred.valid_destination(state, state_model)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl TryFrom<DestinationPredicateConfig> for DestinationPredicate {
    type Error = DestinationError;

    fn try_from(value: DestinationPredicateConfig) -> Result<Self, Self::Error> {
        match value {
            DestinationPredicateConfig::Boolean { feature, negate } => {
                Ok(DestinationPredicate::Boolean { feature, negate })
            }
        }
    }
}

impl std::fmt::Display for DestinationPredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DestinationPredicate::Boolean { feature, negate } => {
                if !negate {
                    format!("{feature}=true")
                } else {
                    format!("{feature}=false")
                }
            }
        };
        write!(f, "{s}")
    }
}

impl DestinationPredicate {
    pub fn valid_destination(
        &self,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, DestinationError> {
        match self {
            DestinationPredicate::Boolean { feature, negate } => {
                let variable = state_model.get_custom_bool(state, feature).map_err(|e| {
                    DestinationError::StateErrorInPredicate {
                        predicate: self.clone(),
                        error: e,
                    }
                })?;
                Ok(variable ^ !negate) // XOR, aka F&F || T&T
            }
        }
    }
}
