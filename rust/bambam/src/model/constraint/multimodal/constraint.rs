use crate::model::constraint::multimodal::constraint_config::{
    EnergyStateVariable, ModeLegDistanceConstraint, ModeLegEnergyConstraint, ModeLegTimeConstraint,
};
use crate::model::constraint::multimodal::sequence_trie::SubSequenceTrie;
use crate::model::constraint::multimodal::{
    multimodal_frontier_ops as ops, ConstraintConfig, DistanceConstraint, EnergyConstraint,
    TimeConstraint,
};
use bambam_core::model::state::{
    multimodal_state_ops as state_ops, CategoricalMapping, CategoricalStateMapping,
};
use bambam_core::model::{bambam_field, bambam_state};
use routee_compass_core::model::state::StateModelError;
use routee_compass_core::model::traversal::default::fieldname;
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Edge,
    state::{StateModel, StateVariable},
    unit::TimeUnit,
};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
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
    /// Set distance limit for the trip.
    DistanceConstraint(DistanceConstraint),
    /// Set time limit for the trip.
    TimeConstraint(TimeConstraint),
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
    /// returns false if this constraint statically forbids `edge_mode` for the entire query.
    pub fn allows_mode(&self, edge_mode: &str, max_trip_legs: NonZeroU64) -> bool {
        use Constraint as MFC;

        match self {
            MFC::AllowedModes(items) => items.contains(edge_mode),
            MFC::ModeCounts(limits) => limits.get(edge_mode).copied().unwrap_or(0) > 0,
            MFC::ExactSequences(trie) => {
                trie.contains_mode(edge_mode, Some(max_trip_legs.get() as usize))
            }
            _ => true,
        }
    }

    /// validates an edge independent of search state (e.g., during map matching).
    pub fn valid_edge(&self, edge_mode: &str, _edge: &Edge) -> Result<bool, ConstraintModelError> {
        use Constraint as MFC;

        match self {
            MFC::AllowedModes(items) => Ok(items.contains(edge_mode)),
            MFC::ModeCounts(limits) => Ok(limits.get(edge_mode).copied().unwrap_or(0) > 0),
            MFC::ExactSequences(trie) => Ok(trie.contains_mode(edge_mode, None)),
            _ => Ok(true),
        }
    }

    /// validates an edge for traversal in a multimodal traversal
    pub fn valid_frontier(
        &self,
        edge_mode: &str,
        edge: &Edge,
        state: &[StateVariable],
        state_model: &StateModel,
        mode_to_state: &CategoricalStateMapping,
        max_trip_legs: NonZeroU64,
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

            MFC::DistanceConstraint(limit) => {
                validate_trip_distance(state, state_model, edge, limit)
            }

            MFC::TimeConstraint(limit) => validate_trip_time(state, state_model, limit),

            MFC::ModeDistanceLimit {
                mode_distance_limit,
            } => validate_mode_distance(
                state,
                state_model,
                edge,
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
                edge,
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
            } => validate_mode_leg_energy(
                state,
                state_model,
                mode_leg_energy_limit,
                max_trip_legs,
                mode_to_state,
                edge_mode,
            ),
        }
    }
}

impl TryFrom<&ConstraintConfig> for Constraint {
    type Error = ConstraintModelError;

    fn try_from(value: &ConstraintConfig) -> Result<Self, Self::Error> {
        use ConstraintConfig as MFCC;
        match value {
            MFCC::AllowedModes { values } => {
                let modes = values.iter().cloned().collect::<HashSet<_>>();
                Ok(Self::AllowedModes(modes))
            }
            MFCC::ModeCounts { values } => {
                let counts = values
                    .iter()
                    .map(|(k, v)| {
                        let v_usize: usize = v.get().try_into().map_err(|e| {
                            ConstraintModelError::ConstraintModelError(format!(
                                "while reading mode count limit: {e}"
                            ))
                        })?;
                        Ok((k.clone(), v_usize))
                    })
                    .collect::<Result<HashMap<_, _>, ConstraintModelError>>()?;
                Ok(Self::ModeCounts(counts))
            }
            MFCC::ExactSequences { values } => {
                let mut trie = SubSequenceTrie::new();
                for seq in values.iter() {
                    trie.insert_sequence(seq.clone());
                }
                Ok(Self::ExactSequences(trie))
            }
            MFCC::DistanceConstraint(c) => Ok(Self::DistanceConstraint(c.clone())),
            MFCC::TimeConstraint(c) => Ok(Self::TimeConstraint(c.clone())),
            MFCC::ModeDistanceLimit(values) => Ok(Self::ModeDistanceLimit {
                mode_distance_limit: values.0.clone(),
            }),
            MFCC::ModeTimeLimit(values) => Ok(Self::ModeTimeLimit {
                mode_time_limit: values.0.clone(),
            }),
            // MFCC::ModeEnergyLimit { mode_energy_limit } => Ok(Self::ModeEnergyLimit {
            //     mode_energy_limit: mode_energy_limit.clone(),
            // }),
            MFCC::ModeLegDistanceLimit(values) => Ok(Self::ModeLegDistanceLimit {
                mode_leg_distance_limit: values.0.clone(),
            }),
            MFCC::ModeLegTimeLimit(values) => Ok(Self::ModeLegTimeLimit {
                mode_leg_time_limit: values.0.clone(),
            }),
            // MFCC::ModeLegEnergyLimit {
            //     mode_leg_energy_limit,
            // } => Ok(Self::ModeLegEnergyLimit {
            //     mode_leg_energy_limit: mode_leg_energy_limit.clone(),
            // }),
        }
    }
}

type ConstraintResult = Result<bool, ConstraintModelError>;

/// runs the constraint model validation logic for mode count constraints
fn validate_mode_counts(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, usize>,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let mut counts = ops::get_mode_counts(state, state_model, max_trip_legs, mode_to_state)?;

    // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
    let active_mode =
        state_ops::get_active_leg_mode(state, state_model, max_trip_legs, mode_to_state).map_err(
            |e| {
                ConstraintModelError::ConstraintModelError(format!(
                    "while applying mode count constraint model, {e}"
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
fn validate_mode_sequences<'a>(
    state: &[StateVariable],
    state_model: &StateModel,
    trie: &SubSequenceTrie,
    max_trip_legs: NonZeroU64,
    mode_to_state: &'a CategoricalMapping<String, i64>,
    edge_mode: &'a str,
) -> ConstraintResult {
    const STACK_CAP: usize = 16;
    let max_legs = max_trip_legs.get() as usize;

    if max_legs + 1 <= STACK_CAP {
        let mut buf: [&'a str; STACK_CAP] = [""; STACK_CAP];
        let mut len = 0usize;
        let mut leg_idx = 0u64;

        while state_ops::contains_leg(state, leg_idx, state_model, max_trip_legs).map_err(|e| {
            ConstraintModelError::ConstraintModelError(format!(
                "while testing for matching mode sub-sequence, had error: {e}"
            ))
        })? {
            let mode = state_ops::get_existing_leg_mode(
                state,
                leg_idx,
                state_model,
                max_trip_legs,
                mode_to_state,
            )
            .map_err(|e| {
                ConstraintModelError::ConstraintModelError(format!(
                    "while testing for matching mode sub-sequence, had error: {e}"
                ))
            })?;
            buf[len] = mode;
            len += 1;
            leg_idx += 1;
        }

        let active_mode = if len > 0 { Some(buf[len - 1]) } else { None };
        if Some(edge_mode) != active_mode {
            buf[len] = edge_mode;
            len += 1;
        }
        return Ok(trie.contains(&buf[..len]));
    }

    let mut modes = state_ops::get_mode_sequence(state, state_model, max_trip_legs, mode_to_state)
        .map_err(|e| {
            ConstraintModelError::ConstraintModelError(format!(
                "while testing for matching mode sub-sequence, had error: {e}"
            ))
        })?;

    // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
    let active_mode = modes.last().copied();
    if Some(edge_mode) != active_mode {
        modes.push(edge_mode);
    }
    let is_match = trie.contains(&modes);
    Ok(is_match)
}

fn validate_trip_distance(
    state: &[StateVariable],
    state_model: &StateModel,
    edge: &Edge,
    limit: &DistanceConstraint,
) -> ConstraintResult {
    let dist: Length = state_model
        .get_distance(state, fieldname::TRIP_DISTANCE)
        .map_err(|e| {
            let msg = format!(
                "while retrieving '{}' from state: {e}",
                fieldname::TRIP_DISTANCE
            );
            ConstraintModelError::ConstraintModelError(msg)
        })?;
    let valid = limit.test(dist + edge.distance, false);
    Ok(valid)
}

fn validate_trip_time(
    state: &[StateVariable],
    state_model: &StateModel,
    limit: &TimeConstraint,
) -> ConstraintResult {
    let time: Time = state_model
        .get_time(state, fieldname::TRIP_TIME)
        .map_err(|e| {
            let msg = format!(
                "while retrieving '{}' from state: {e}",
                fieldname::TRIP_TIME
            );
            ConstraintModelError::ConstraintModelError(msg)
        })?;
    let valid = limit.test(time, false);
    log::debug!(
        "validating trip_time with time {:.2} minutes against limit {limit:?}. valid? {valid}",
        time.get::<uom::si::time::minute>(),
    );
    Ok(valid)
}

/// runs the constraint model validation logic for mode distance constraints
fn validate_mode_distance(
    state: &[StateVariable],
    state_model: &StateModel,
    edge: &Edge,
    limits: &HashMap<String, DistanceConstraint>,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let active_mode = get_active_mode(state, state_model, max_trip_legs, mode_to_state)?;
    let ending_leg = active_mode.is_some_and(|leg| leg != edge_mode);

    // if switching modes, validate any minimum distance constraint on the ending leg's mode
    if ending_leg {
        if let Some(prev_mode) = active_mode {
            if let Some(constraint) = limits.get(prev_mode) {
                if constraint.has_min() {
                    let value: Length = get_mode_distance(prev_mode, state, state_model)?;
                    if !constraint.test_min(value) {
                        return Ok(false);
                    }
                }
            }
        }
    }

    // validate any maximum distance constraint on the incoming edge's mode
    if let Some(constraint) = limits.get(edge_mode) {
        if constraint.has_max() {
            let value: Length = get_mode_distance(edge_mode, state, state_model)?;
            if !constraint.test_max(value + edge.distance) {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// runs the constraint model validation logic for mode time constraints
fn validate_mode_time(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, TimeConstraint>,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let active_mode = get_active_mode(state, state_model, max_trip_legs, mode_to_state)?;
    let ending_leg = active_mode.is_some_and(|leg| leg != edge_mode);

    // if switching modes, validate any minimum time constraint on the ending leg's mode
    if ending_leg {
        if let Some(prev_mode) = active_mode {
            if let Some(constraint) = limits.get(prev_mode) {
                if constraint.has_min() {
                    let value: Time = get_mode_time(prev_mode, state, state_model)?;
                    if !constraint.test_min(value) {
                        return Ok(false);
                    }
                }
            }
        }
    }

    // validate any maximum time constraint on the incoming edge's mode
    if let Some(constraint) = limits.get(edge_mode) {
        if constraint.has_max() {
            let value: Time = get_mode_time(edge_mode, state, state_model)?;
            if !constraint.test_max(value) {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// runs the constraint model validation logic for mode energy constraints
fn validate_mode_energy(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, EnergyConstraint>,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let active_mode = get_active_mode(state, state_model, max_trip_legs, mode_to_state)?;
    let ending_leg = active_mode.is_some_and(|leg| leg != edge_mode);

    if ending_leg {
        if let Some(prev_mode) = active_mode {
            if let Some(constraint) = limits.get(prev_mode) {
                if constraint.has_min() {
                    let value: Energy = get_mode_energy(&constraint.variable, state, state_model)?;
                    if !constraint.test_min(value) {
                        return Ok(false);
                    }
                }
            }
        }
    }

    if let Some(constraint) = limits.get(edge_mode) {
        if constraint.has_max() {
            let value: Energy = get_mode_energy(&constraint.variable, state, state_model)?;
            if !constraint.test_max(value) {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// runs the constraint model validation logic for mode leg distance constraints
fn validate_mode_leg_distance(
    state: &[StateVariable],
    state_model: &StateModel,
    edge: &Edge,
    limits: &HashMap<String, ModeLegDistanceConstraint>,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let active_leg_idx = state_ops::get_active_leg_idx(state, state_model).map_err(|e| {
        ConstraintModelError::ConstraintModelError(format!(
            "while validating mode leg distance, {e}"
        ))
    })?;
    let active_mode = get_active_mode(state, state_model, max_trip_legs, mode_to_state)?;
    let ending_leg = active_mode.is_some_and(|leg| leg != edge_mode);

    // 1. If we are switching modes, check any minimum leg distance constraint on the leg that is ending
    if ending_leg {
        if let Some(prev_mode) = active_mode {
            if let Some(ModeLegDistanceConstraint { leg, constraint }) = limits.get(prev_mode) {
                if constraint.has_min()
                    && leg.matches_leg_idx(active_leg_idx, state, state_model, max_trip_legs)?
                {
                    if let Some(value) = get_active_leg_distance(state, state_model)? {
                        if !constraint.test_min(value) {
                            return Ok(false);
                        }
                    }
                }
            }
        }
    }

    // 2. Check any maximum leg distance constraint on the incoming edge's mode
    if let Some(ModeLegDistanceConstraint { leg, constraint }) = limits.get(edge_mode) {
        if constraint.has_max() {
            let target_leg_idx = match (active_leg_idx, ending_leg) {
                (None, _) => Some(0),
                (Some(idx), true) => Some(idx + 1),
                (Some(idx), false) => Some(idx),
            };
            if leg.matches_leg_idx(target_leg_idx, state, state_model, max_trip_legs)? {
                let value = if active_mode == Some(edge_mode) {
                    get_active_leg_distance(state, state_model)?.unwrap_or_default()
                } else {
                    Length::default()
                };
                if !constraint.test_max(value + edge.distance) {
                    return Ok(false);
                }
            }
        }
    }

    Ok(true)
}

/// runs the constraint model validation logic for mode leg time constraints
fn validate_mode_leg_time(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, ModeLegTimeConstraint>,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let active_leg_idx = state_ops::get_active_leg_idx(state, state_model).map_err(|e| {
        ConstraintModelError::ConstraintModelError(format!("while validating mode leg time, {e}"))
    })?;
    let active_mode = get_active_mode(state, state_model, max_trip_legs, mode_to_state)?;
    let ending_leg = active_mode.is_some_and(|leg| leg != edge_mode);

    // 1. If we are switching modes, check any minimum leg time constraint on the leg that is ending
    if ending_leg {
        if let Some(prev_mode) = active_mode {
            if let Some(ModeLegTimeConstraint { leg, constraint }) = limits.get(prev_mode) {
                if constraint.has_min()
                    && leg.matches_leg_idx(active_leg_idx, state, state_model, max_trip_legs)?
                {
                    if let Some(value) = get_active_leg_time(state, state_model)? {
                        if !constraint.test_min(value) {
                            return Ok(false);
                        }
                    }
                }
            }
        }
    }

    // 2. Check any maximum leg time constraint on the incoming edge's mode
    if let Some(ModeLegTimeConstraint { leg, constraint }) = limits.get(edge_mode) {
        if constraint.has_max() {
            let target_leg_idx = match (active_leg_idx, ending_leg) {
                (None, _) => Some(0),
                (Some(idx), true) => Some(idx + 1),
                (Some(idx), false) => Some(idx),
            };
            if leg.matches_leg_idx(target_leg_idx, state, state_model, max_trip_legs)? {
                let value = if active_mode == Some(edge_mode) {
                    get_active_leg_time(state, state_model)?.unwrap_or_default()
                } else {
                    Time::default()
                };
                if !constraint.test_max(value) {
                    return Ok(false);
                }
            }
        }
    }

    Ok(true)
}

/// runs the constraint model validation logic for mode leg energy constraints
fn validate_mode_leg_energy(
    state: &[StateVariable],
    state_model: &StateModel,
    limits: &HashMap<String, ModeLegEnergyConstraint>,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> ConstraintResult {
    let active_leg_idx = state_ops::get_active_leg_idx(state, state_model).map_err(|e| {
        ConstraintModelError::ConstraintModelError(format!("while validating mode leg energy, {e}"))
    })?;
    let active_mode = get_active_mode(state, state_model, max_trip_legs, mode_to_state)?;
    let ending_leg = active_mode.is_some_and(|leg| leg != edge_mode);

    if ending_leg {
        if let Some(prev_mode) = active_mode {
            if let Some(ModeLegEnergyConstraint { leg, constraint }) = limits.get(prev_mode) {
                if constraint.has_min()
                    && leg.matches_leg_idx(active_leg_idx, state, state_model, max_trip_legs)?
                {
                    if let Some(value) = get_active_leg_energy(state, state_model)? {
                        if !constraint.test_min(value) {
                            return Ok(false);
                        }
                    }
                }
            }
        }
    }

    if let Some(ModeLegEnergyConstraint { leg, constraint }) = limits.get(edge_mode) {
        if constraint.has_max() {
            let target_leg_idx = match (active_leg_idx, ending_leg) {
                (None, _) => Some(0),
                (Some(idx), true) => Some(idx + 1),
                (Some(idx), false) => Some(idx),
            };
            if leg.matches_leg_idx(target_leg_idx, state, state_model, max_trip_legs)? {
                let value = if active_mode == Some(edge_mode) {
                    get_active_leg_energy(state, state_model)?.unwrap_or_default()
                } else {
                    Energy::default()
                };
                if !constraint.test_max(value) {
                    return Ok(false);
                }
            }
        }
    }

    Ok(true)
}

/// helper for retrieving distance values from the state vector.
fn get_mode_distance(
    edge_mode: &str,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Length, ConstraintModelError> {
    state_ops::get_mode_distance(state, edge_mode, state_model).map_err(|e| {
        let msg = format!("while retrieving '{edge_mode}' mode distance from state: {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })
}

/// helper for retrieving time values from the state vector.
fn get_mode_time(
    edge_mode: &str,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Time, ConstraintModelError> {
    state_ops::get_mode_time(state, edge_mode, state_model).map_err(|e| {
        let msg = format!("while retrieving '{edge_mode}' mode time from state: {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })
}

/// helper for retrieving all energy values from the state vector based on the constraint's
/// expected energy fieldnames.
fn get_mode_energy(
    variable: &EnergyStateVariable,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Energy, ConstraintModelError> {
    todo!("add a state_ops::get_mode_energy + update multimodal traversal model accounting for energy")
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

/// inspect the active trip mode for this state.
fn get_active_mode<'a>(
    state: &[StateVariable],
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
    mode_to_state: &'a CategoricalMapping<String, i64>,
) -> Result<Option<&'a str>, ConstraintModelError> {
    state_ops::get_active_leg_mode(state, state_model, max_trip_legs, mode_to_state).map_err(|e| {
        ConstraintModelError::ConstraintModelError(format!(
            "while checking active leg mode in metric-based constraint, {e}"
        ))
    })
}

/// inspect the active trip mode and this edge's travel mode. if they do not match,
/// then we are ending a leg by making this transition.
fn check_mode_switch(
    state: &[StateVariable],
    state_model: &StateModel,
    max_trip_legs: NonZeroU64,
    mode_to_state: &CategoricalMapping<String, i64>,
    edge_mode: &str,
) -> Result<bool, ConstraintModelError> {
    let active_leg = get_active_mode(state, state_model, max_trip_legs, mode_to_state)?;
    match active_leg {
        None => Ok(false),
        Some(leg) => Ok(leg != edge_mode),
    }
}

fn get_active_leg_distance(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Option<Length>, ConstraintModelError> {
    let leg_idx_opt = state_ops::get_active_leg_idx(state, state_model).map_err(|e| {
        let msg = format!("while validating mode leg distance, {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    let leg_idx = match leg_idx_opt {
        None => return Ok(None), // we haven't started the trip yet
        Some(idx) => idx,
    };
    let value: Length = state_ops::get_leg_distance(state, leg_idx, state_model).map_err(|e| {
        let msg = format!("while validating mode leg distance, {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    Ok(Some(value))
}

fn get_active_leg_time(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Option<Time>, ConstraintModelError> {
    let leg_idx_opt = state_ops::get_active_leg_idx(state, state_model).map_err(|e| {
        let msg = format!("while validating mode leg time, {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    let leg_idx = match leg_idx_opt {
        None => return Ok(None), // we haven't started the trip yet
        Some(idx) => idx,
    };
    let value: Time = state_ops::get_leg_time(state, leg_idx, state_model).map_err(|e| {
        let msg = format!("while validating mode leg time, {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    Ok(Some(value))
}

fn get_active_leg_energy(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<Option<Energy>, ConstraintModelError> {
    let leg_idx_opt = state_ops::get_active_leg_idx(state, state_model).map_err(|e| {
        let msg = format!("while validating mode leg energy, {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    let leg_idx = match leg_idx_opt {
        None => return Ok(None), // we haven't started the trip yet
        Some(idx) => idx,
    };
    let value: Energy = state_ops::get_leg_energy(state, leg_idx, state_model).map_err(|e| {
        let msg = format!("while validating mode leg energy, {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    Ok(Some(value))
}
