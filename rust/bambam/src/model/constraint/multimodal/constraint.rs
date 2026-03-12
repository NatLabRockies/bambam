use crate::model::constraint::multimodal::constraint_config::{
    EnergyStateVariable, ModeLegDistanceConstraint, ModeLegEnergyConstraint, ModeLegTimeConstraint,
};
use crate::model::constraint::multimodal::sequence_trie::SubSequenceTrie;
use crate::model::constraint::multimodal::{
    multimodal_frontier_ops as ops, ConstraintConfig, DistanceConstraint, EnergyConstraint,
    TimeConstraint,
};
use crate::model::state::{
    multimodal_state_ops as state_ops, MultimodalMapping, MultimodalStateMapping,
};
use bambam_core::model::{bambam_field, bambam_state};
use routee_compass_core::model::state::StateModelError;
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Edge,
    state::{StateModel, StateVariable},
    unit::TimeUnit,
};
use std::collections::{HashMap, HashSet};
use uom::si::f64::{Energy, Length, Time};

#[derive(Debug)]
/// types of constraints to limit exponential search expansion in multimodal scenarios.
///
/// only deals with constraints associated with multimodal metadata, since metric-based
/// constraints must be applied _after_ access + traversal metrics have been run.
pub enum Constraint {
    /// Restrict routes to only use allowed transportation modes.
    AllowedModes(HashSet<String>),
    /// Limit the number of times each mode can be used in a route.
    ModeCounts(HashMap<String, usize>),
    /// Require routes to follow one of the specified mode sequences.
    ExactSequences(SubSequenceTrie),
    /// Set maximum distance limits for each transportation mode.
    ModeDistanceLimit {
        mode_distance_limit: HashMap<String, DistanceConstraint>,
    },
    /// Set maximum time limits for each transportation mode.
    ModeTimeLimit {
        mode_time_limit: HashMap<String, TimeConstraint>,
    },
    /// Set maximum energy limits for each transportation mode.
    ModeEnergyLimit {
        mode_energy_limit: HashMap<String, EnergyConstraint>,
    },
    /// Set distance limits for specific modes on specific trip legs.
    ModeLegDistanceLimit {
        mode_leg_distance_limit: HashMap<String, ModeLegDistanceConstraint>,
    },
    /// Set time limits for specific modes on specific trip legs.
    ModeLegTimeLimit {
        mode_leg_time_limit: HashMap<String, ModeLegTimeConstraint>,
    },
    /// Set energy limits for specific modes on specific trip legs.
    ModeLegEnergyLimit {
        mode_leg_energy_limit: HashMap<String, ModeLegEnergyConstraint>,
    },
}

impl Constraint {
    /// validates an edge for traversal in a multimodal traversal
    pub fn valid_frontier(
        &self,
        edge_mode: &str,
        edge: &Edge,
        state: &[StateVariable],
        state_model: &StateModel,
        mode_to_state: &MultimodalStateMapping,
        max_trip_legs: u64,
    ) -> Result<bool, ConstraintModelError> {
        use Constraint as MFC;

        match self {
            MFC::AllowedModes(items) => {
                let result = items.contains(edge_mode);
                Ok(result)
            }

            MFC::ModeCounts(limits) => validate_mode_counts(
                state,
                state_model,
                limits,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),

            MFC::ExactSequences(trie) => validate_mode_sequences(
                state,
                state_model,
                trie,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),

            MFC::ModeDistanceLimit {
                mode_distance_limit,
            } => validate_mode_distance(
                state,
                state_model,
                mode_distance_limit,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),

            MFC::ModeTimeLimit { mode_time_limit } => validate_mode_time(
                state,
                state_model,
                mode_time_limit,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),
            MFC::ModeEnergyLimit { mode_energy_limit } => validate_mode_energy(
                state,
                state_model,
                mode_energy_limit,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),
            MFC::ModeLegDistanceLimit {
                mode_leg_distance_limit,
            } => validate_mode_leg_distance(
                state,
                state_model,
                mode_leg_distance_limit,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),
            MFC::ModeLegTimeLimit {
                mode_leg_time_limit,
            } => validate_mode_leg_time(
                state,
                state_model,
                mode_leg_time_limit,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),
            MFC::ModeLegEnergyLimit {
                mode_leg_energy_limit,
            } => todo!(),
        }
    }
}

impl TryFrom<&ConstraintConfig> for Constraint {
    type Error = ConstraintModelError;

    fn try_from(value: &ConstraintConfig) -> Result<Self, Self::Error> {
        use ConstraintConfig as MFCC;
        match value {
            MFCC::AllowedModes { allowed_modes } => {
                let modes = allowed_modes.iter().cloned().collect::<HashSet<_>>();
                Ok(Self::AllowedModes(modes))
            }
            MFCC::ModeCounts { mode_counts } => {
                let counts = mode_counts
                    .iter()
                    .map(|(k, v)| {
                        let v_usize: usize = v.get().try_into().map_err(|e| {
                            ConstraintModelError::ConstraintModelError(format!(
                                "while reading mode count limit: {e}"
                            ))
                        })?;
                        Ok((k.clone(), v_usize))
                    })
                    .collect::<Result<HashMap<_, _>, _>>()?;
                Ok(Self::ModeCounts(counts))
            }
            MFCC::ExactSequences { exact_sequences } => {
                let mut trie = SubSequenceTrie::new();
                for seq in exact_sequences.iter() {
                    trie.insert_sequence(seq.clone());
                }
                Ok(Self::ExactSequences(trie))
            }
            MFCC::ModeDistanceLimit {
                mode_distance_limit,
            } => Ok(Self::ModeDistanceLimit {
                mode_distance_limit: mode_distance_limit.clone(),
            }),
            MFCC::ModeTimeLimit { mode_time_limit } => Ok(Self::ModeTimeLimit {
                mode_time_limit: mode_time_limit.clone(),
            }),
            MFCC::ModeEnergyLimit { mode_energy_limit } => Ok(Self::ModeEnergyLimit {
                mode_energy_limit: mode_energy_limit.clone(),
            }),
            MFCC::ModeLegDistanceLimit {
                mode_leg_distance_limit,
            } => Ok(Self::ModeLegDistanceLimit {
                mode_leg_distance_limit: mode_leg_distance_limit.clone(),
            }),
            MFCC::ModeLegTimeLimit {
                mode_leg_time_limit,
            } => Ok(Self::ModeLegTimeLimit {
                mode_leg_time_limit: mode_leg_time_limit.clone(),
            }),
            MFCC::ModeLegEnergyLimit {
                mode_leg_energy_limit: mode_leg_distance_limit,
            } => Ok(Self::ModeLegEnergyLimit {
                mode_leg_energy_limit: mode_leg_distance_limit.clone(),
            }),
        }
    }
}

type ConstraintResult = Result<bool, ConstraintModelError>;

/// runs the constraint model validation logic for mode count constraints
fn validate_mode_counts(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, usize>,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let mut counts = ops::get_mode_counts(state, state_model, max_trip_legs, mode_to_state)?;

    // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
    let active_mode =
        state_ops::get_active_leg_mode(state, state_model, max_trip_legs, mode_to_state).map_err(
            |e| {
                ConstraintModelError::ConstraintModelError(format!(
                    "while applying mode count frontier model constraint, {e}"
                ))
            },
        )?;
    if Some(edge_mode) != active_mode {
        counts
            .entry(edge_mode.to_string())
            .and_modify(|cnt| *cnt += 1)
            .or_insert(1);
    }

    Ok(ops::valid_mode_counts(&counts, limits))
}

/// runs the constraint model validation logic for exact sequence constraints
fn validate_mode_sequences(
    state: &[StateVariable],
    state_model: &StateModel,
    trie: &SubSequenceTrie,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let mut modes = state_ops::get_mode_sequence(state, state_model, max_trip_legs, mode_to_state)
        .map_err(|e| {
            ConstraintModelError::ConstraintModelError(format!(
                "while testing for matching mode sub-sequence, had error: {e}"
            ))
        })?;

    // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
    let active_mode =
        state_ops::get_active_leg_mode(state, state_model, max_trip_legs, mode_to_state).map_err(
            |e| {
                ConstraintModelError::ConstraintModelError(format!(
                    "while applying mode count frontier model constraint, {e}"
                ))
            },
        )?;
    if Some(edge_mode) != active_mode {
        modes.push(edge_mode.to_string());
    }
    let is_match = trie.contains(&modes);
    Ok(is_match)
}

/// runs the constraint model validation logic for mode distance constraints
fn validate_mode_distance(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, DistanceConstraint>,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    match limits.get(edge_mode) {
        Some(constraint) => {
            let value: Length = get_distance(bambam_state::TRIP_DISTANCE, state, state_model)?;
            let ending_leg =
                check_mode_switch(state, state_model, max_trip_legs, mode_to_state, edge_mode)?;
            let valid = constraint.test(value, ending_leg);
            Ok(valid)
        }
        None => Ok(true),
    }
}

/// runs the constraint model validation logic for mode distance constraints
fn validate_mode_time(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, TimeConstraint>,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    match limits.get(edge_mode) {
        Some(constraint) => {
            let value = state_model
                .get_time(state, bambam_state::TRIP_TIME)
                .map_err(|e| {
                    let msg = format!(
                        "while retrieving {} from state: {e}",
                        bambam_state::TRIP_TIME
                    );
                    ConstraintModelError::ConstraintModelError(msg)
                })?;
            let ending_leg =
                check_mode_switch(state, state_model, max_trip_legs, mode_to_state, edge_mode)?;
            let valid = constraint.test(value, ending_leg);
            Ok(valid)
        }
        None => Ok(true),
    }
}

/// runs the constraint model validation logic for mode distance constraints
fn validate_mode_energy(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, EnergyConstraint>,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    use bambam_state::{TRIP_ENERGY_ELECTRIC, TRIP_ENERGY_LIQUID};
    match limits.get(edge_mode) {
        Some(constraint) => {
            let value: Energy = get_total_energy(&constraint.variable, state, state_model)?;
            let ending_leg =
                check_mode_switch(state, state_model, max_trip_legs, mode_to_state, edge_mode)?;
            let valid = constraint.test(value, ending_leg);
            Ok(valid)
        }
        None => Ok(true),
    }
}

/// runs the constraint model validation logic for mode distance constraints
fn validate_mode_leg_distance(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, ModeLegDistanceConstraint>,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    match limits.get(edge_mode) {
        Some(ModeLegDistanceConstraint { leg, constraint }) => {
            let matches = leg.matches(state, state_model, max_trip_legs as usize)?;
            if !matches {
                return Ok(true);
            }
            let value: Length = get_distance(bambam_state::TRIP_DISTANCE, state, state_model)?;
            let ending_leg =
                check_mode_switch(state, state_model, max_trip_legs, mode_to_state, edge_mode)?;
            let valid = constraint.test(value, ending_leg);
            Ok(valid)
        }
        None => Ok(true),
    }
}

/// runs the constraint model validation logic for mode time constraints
fn validate_mode_leg_time(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, ModeLegTimeConstraint>,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    match limits.get(edge_mode) {
        Some(ModeLegTimeConstraint { leg, constraint }) => {
            let matches = leg.matches(state, state_model, max_trip_legs as usize)?;
            if !matches {
                return Ok(true);
            }
            let value: Time = get_time(bambam_state::TRIP_TIME, state, state_model)?;
            let ending_leg =
                check_mode_switch(state, state_model, max_trip_legs, mode_to_state, edge_mode)?;
            let valid = constraint.test(value, ending_leg);
            Ok(valid)
        }
        None => Ok(true),
    }
}

/// runs the constraint model validation logic for mode time constraints
fn validate_mode_leg_energy(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, ModeLegEnergyConstraint>,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    match limits.get(edge_mode) {
        Some(ModeLegEnergyConstraint { leg, constraint }) => {
            let matches = leg.matches(state, state_model, max_trip_legs as usize)?;
            if !matches {
                return Ok(true);
            }
            let value: Energy = get_total_energy(&constraint.variable, state, state_model)?;
            let mode_switch =
                check_mode_switch(state, state_model, max_trip_legs, mode_to_state, edge_mode)?;
            let valid = constraint.test(value, mode_switch);
            Ok(valid)
        }
        None => Ok(true),
    }
}

/// helper for retrieving distance values from the state vector.
fn get_distance(
    fieldname: &str,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Length, ConstraintModelError> {
    state_model.get_distance(state, fieldname).map_err(|e| {
        let msg = format!("while retrieving {} from state: {e}", fieldname);
        ConstraintModelError::ConstraintModelError(msg)
    })
}

/// helper for retrieving time values from the state vector.
fn get_time(
    fieldname: &str,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Time, ConstraintModelError> {
    state_model.get_time(state, fieldname).map_err(|e| {
        let msg = format!("while retrieving {} from state: {e}", fieldname);
        ConstraintModelError::ConstraintModelError(msg)
    })
}

/// helper for retrieving all energy values from the state vector based on the constraint's
/// expected energy fieldnames.
fn get_total_energy(
    variable: &EnergyStateVariable,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Energy, ConstraintModelError> {
    use bambam_state::{TRIP_ENERGY_ELECTRIC, TRIP_ENERGY_LIQUID};
    match variable {
        EnergyStateVariable::Liquid => get_energy(TRIP_ENERGY_LIQUID, state, state_model),
        EnergyStateVariable::Electric => get_energy(TRIP_ENERGY_ELECTRIC, state, state_model),
        EnergyStateVariable::Both => {
            let liq: Energy = get_energy(TRIP_ENERGY_LIQUID, state, state_model)?;
            let ele: Energy = get_energy(TRIP_ENERGY_ELECTRIC, state, state_model)?;
            Ok(liq + ele)
        }
    }
}

/// helper for retrieving energy values from the state vector.
fn get_energy(
    fieldname: &str,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Energy, ConstraintModelError> {
    state_model.get_energy(state, fieldname).map_err(|e| {
        let msg = format!("while retrieving {} from state: {e}", fieldname);
        ConstraintModelError::ConstraintModelError(msg)
    })
}

/// inspect the active trip mode and this edge's travel mode. if they do not match,
/// then we are ending a leg by making this transition.
fn check_mode_switch(
    state: &[StateVariable],
    state_model: &StateModel,
    max_trip_legs: u64,
    mode_to_state: &MultimodalMapping<String, i64>,
    edge_mode: &str,
) -> Result<bool, ConstraintModelError> {
    let active_leg =
        state_ops::get_active_leg_mode(state, state_model, max_trip_legs, mode_to_state).map_err(
            |e| {
                ConstraintModelError::ConstraintModelError(format!(
                    "while applying mode count frontier model constraint, {e}"
                ))
            },
        )?;
    match active_leg {
        None => Ok(false),
        Some(leg) => Ok(leg != edge_mode),
        _ => Ok(false),
    }
}
