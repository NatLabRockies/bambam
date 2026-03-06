use routee_compass_core::model::state::{StateModel, StateVariable};
use serde::{Deserialize, Serialize};

use crate::model::destination::DestinationError;

/// filter(s) to apply while collecting destinations. these are applied
/// regardless of any binning configuration.
#[derive(Clone, Debug)]
pub struct DestinationFilter(pub Vec<DestinationPredicate>);

/// additional modifiers to apply when collecting destinations for a bin.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum DestinationPredicate {
    /// only accept destinations where the provided feature, a boolean value,
    /// is true (or, if negate == true, where the feature is false).
    Boolean {
        /// state variable feature to match
        feature: String,
        /// if true, invert the value stored at the feature    
        negate: bool,
    },
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
                Ok(variable != *negate) // if negate=false, variable should be true, and vice versa
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use routee_compass_core::model::state::{CustomVariableConfig, StateVariableConfig};

    use super::*;

    fn mock(state_variables: &[(&str, bool)]) -> (StateModel, Vec<StateVariable>) {
        let features = state_variables
            .iter()
            .map(|(name, val)| {
                (
                    name.to_string(),
                    StateVariableConfig::Custom {
                        custom_type: "bool".to_string(),
                        value: CustomVariableConfig::Boolean { initial: *val },
                        accumulator: false,
                    },
                )
            })
            .collect_vec();
        let model = StateModel::new(features);
        let initial = model.initial_state(None).expect("test invariant failed");
        (model, initial)
    }

    #[test]
    fn test_valid_destination_boolean_true_negate_false() {
        // When feature is true and negate is false, should return true
        let predicate = DestinationPredicate::Boolean {
            feature: "is_available".to_string(),
            negate: false,
        };

        let (state_model, state) = mock(&[("is_available", true)]);
        let result = predicate
            .valid_destination(&state, &state_model)
            .expect("test invariant failed");
        assert_eq!(result, true);
    }

    #[test]
    fn test_valid_destination_boolean_true_negate_true() {
        // When feature is true and negate is true, should return false
        let predicate = DestinationPredicate::Boolean {
            feature: "is_available".to_string(),
            negate: true,
        };

        let (state_model, state) = mock(&[("is_available", true)]);
        let result = predicate
            .valid_destination(&state, &state_model)
            .expect("test invariant failed");
        assert_eq!(result, false);
    }

    #[test]
    fn test_valid_destination_boolean_false_negate_false() {
        // When feature is false and negate is false, should return false
        let predicate = DestinationPredicate::Boolean {
            feature: "is_available".to_string(),
            negate: false,
        };

        let (state_model, state) = mock(&[("is_available", false)]);
        let result = predicate
            .valid_destination(&state, &state_model)
            .expect("test invariant failed");
        assert_eq!(result, false);
    }

    #[test]
    fn test_valid_destination_boolean_false_negate_true() {
        // When feature is false and negate is true, should return true
        let predicate = DestinationPredicate::Boolean {
            feature: "is_available".to_string(),
            negate: true,
        };

        let (state_model, state) = mock(&[("is_available", false)]);
        let result = predicate
            .valid_destination(&state, &state_model)
            .expect("test invariant failed");
        assert_eq!(result, true);
    }

    #[test]
    fn test_filter_checks_only_specified_variable() {
        // Filter should only check the specified feature and ignore other variables
        let predicate = DestinationPredicate::Boolean {
            feature: "is_available".to_string(),
            negate: false,
        };
        let filter = DestinationFilter(vec![predicate]);

        let (state_model, state) = mock(&[("is_available", true), ("is_active", true)]);
        let result = filter
            .valid_destination(&state, &state_model)
            .expect("test invariant failed");
        assert_eq!(result, true);
    }

    #[test]
    fn test_multiple_filters_all_must_pass() {
        // Multiple predicates in a filter should all pass for the filter to return true
        let pred1 = DestinationPredicate::Boolean {
            feature: "is_available".to_string(),
            negate: false,
        };
        let pred2 = DestinationPredicate::Boolean {
            feature: "is_active".to_string(),
            negate: false,
        };
        let filter = DestinationFilter(vec![pred1, pred2]);

        let (state_model, state) = mock(&[("is_available", true), ("is_active", true)]);
        let result = filter
            .valid_destination(&state, &state_model)
            .expect("test invariant failed");
        assert_eq!(result, true);
    }

    #[test]
    fn test_multiple_filters_one_fails() {
        // If one predicate fails, the entire filter should fail
        let pred1 = DestinationPredicate::Boolean {
            feature: "is_available".to_string(),
            negate: false,
        };
        let pred2 = DestinationPredicate::Boolean {
            feature: "is_active".to_string(),
            negate: false,
        };
        let filter = DestinationFilter(vec![pred1, pred2]);

        let (state_model, state) = mock(&[("is_available", true), ("is_active", false)]);
        let result = filter
            .valid_destination(&state, &state_model)
            .expect("test invariant failed");
        assert_eq!(result, false);
    }
}
