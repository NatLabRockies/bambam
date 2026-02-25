use crate::model::constraint::multimodal::sequence_trie::SubSequenceTrie;
use crate::model::constraint::multimodal::{multimodal_frontier_ops as ops, ConstraintConfig};
use crate::model::state::{
    multimodal_state_ops as state_ops, MultimodalMapping, MultimodalStateMapping,
};
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Edge,
    state::{StateModel, StateVariable},
    unit::TimeUnit,
};
use std::collections::{HashMap, HashSet};
use uom::si::f64::Time;

#[derive(Debug)]
/// types of constraints to limit exponential search expansion in multimodal scenarios.
///
/// only deals with constraints associated with multimodal metadata, since metric-based
/// constraints must be applied _after_ access + traversal metrics have been run.
pub enum Constraint {
    AllowedModes(HashSet<String>),
    ModeCounts(HashMap<String, usize>),
    MaxTripLegs(usize),
    ExactSequences(SubSequenceTrie), // MaxTime(HashMap<String, Time>),
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
            MFC::ModeCounts(limits) => {
                let mut counts =
                    ops::get_mode_counts(state, state_model, max_trip_legs, mode_to_state)?;

                // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
                let active_mode = state_ops::get_active_leg_mode(
                    state,
                    state_model,
                    max_trip_legs,
                    mode_to_state,
                )
                .map_err(|e| {
                    ConstraintModelError::ConstraintModelError(format!(
                        "while applying mode count frontier model constraint, {e}"
                    ))
                })?;
                if Some(edge_mode) != active_mode {
                    counts
                        .entry(edge_mode.to_string())
                        .and_modify(|cnt| *cnt += 1)
                        .or_insert(1);
                }

                Ok(ops::valid_mode_counts(&counts, limits))
            }
            MFC::MaxTripLegs(max_legs) => {
                // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
                let active_mode = state_ops::get_active_leg_mode(
                    state,
                    state_model,
                    max_trip_legs,
                    mode_to_state,
                )
                .map_err(|e| {
                    ConstraintModelError::ConstraintModelError(format!(
                        "while applying mode count frontier model constraint, {e}"
                    ))
                })?;
                let n_existing_legs = state_ops::get_n_legs(state, state_model).map_err(|e| {
                    ConstraintModelError::ConstraintModelError(
                        (format!("while getting number of trip legs for this trip: {e}")),
                    )
                })?;
                let n_legs = match active_mode {
                    Some(active_mode) if active_mode != edge_mode => n_existing_legs + 1,
                    _ => n_existing_legs,
                };
                let is_valid = n_legs <= *max_legs;
                Ok(is_valid)
            }
            MFC::ExactSequences(trie) => {
                let mut modes =
                    state_ops::get_mode_sequence(state, state_model, max_trip_legs, mode_to_state)
                        .map_err(|e| {
                            ConstraintModelError::ConstraintModelError(format!(
                                "while testing for matching mode sub-sequence, had error: {e}"
                            ))
                        })?;

                // simulate a mode transition if the incoming edge has a different mode than the trip's active mode
                let active_mode = state_ops::get_active_leg_mode(
                    state,
                    state_model,
                    max_trip_legs,
                    mode_to_state,
                )
                .map_err(|e| {
                    ConstraintModelError::ConstraintModelError(format!(
                        "while applying mode count frontier model constraint, {e}"
                    ))
                })?;
                if Some(edge_mode) != active_mode {
                    modes.push(edge_mode.to_string());
                }
                let is_match = trie.contains(&modes);
                Ok(is_match)
            }
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
            MFCC::TripLegCount { trip_leg_count } => {
                let max_usize: usize = trip_leg_count.get().try_into().map_err(|e| {
                    ConstraintModelError::ConstraintModelError(format!(
                        "while reading max trip leg limit: {e}"
                    ))
                })?;
                Ok(Self::MaxTripLegs(max_usize))
            }
            MFCC::ExactSequences { exact_sequences } => {
                let mut trie = SubSequenceTrie::new();
                for seq in exact_sequences.iter() {
                    trie.insert_sequence(seq.clone());
                }
                Ok(Self::ExactSequences(trie))
            }
        }
    }
}

// MultimodalConstraint::MaxTime(limits) => {
//     ops::valid_mode_time(state, state_model, limits)
// }
