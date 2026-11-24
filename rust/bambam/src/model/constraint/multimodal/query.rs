use std::{num::NonZeroU64, sync::Once};

use bambam_core::model::state::CategoricalStateMapping;
use itertools::Itertools;
use routee_compass_core::model::constraint::ConstraintModelError;
use serde::{Deserialize, Serialize};

use crate::model::constraint::multimodal::{
    constraint_config::{
        DistanceConstraint, EnergyConstraint, ModeLegDistanceConstraint, ModeLegTimeConstraint,
        TimeConstraint, TripLegConstraint,
    },
    Constraint, ConstraintConfig,
};

/// query-time arguments to the multimodal constraint model
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MultimodalConstraintModelQuery {
    /// constraints to apply when in this mode. without constraints, the multimodal search
    /// space quickly grows intractable and will produce unrealistic behaviors.
    constraints: Option<Vec<ConstraintConfig>>,
    /// each mode transition results in a new trip leg. this value restricts
    /// the number of allowed mode transitions. this is both a domain-specific
    /// configuration value to limit to realistic mode usage and also an algorithmic
    /// configuration value as space complexity grows k^n for k modes, n legs.
    ///
    /// if omitted, inferred from `exact_sequences` (max sequence length) or defaults to 1.
    #[serde(default)]
    pub max_trip_legs: Option<NonZeroU64>,
}

/// use 1 trip leg by default.
pub fn unimodal_trip() -> NonZeroU64 {
    NonZeroU64::MIN
}

/// tracks whether to log (once) the warning about empty constraints on queries.
static EMPTY_CONSTRAINTS_WARNING: Once = Once::new();

impl MultimodalConstraintModelQuery {
    /// resolves `max_trip_legs`, using the explicit value if provided, or inferring it from
    /// `exact_sequences` (longest sequence length) if present, otherwise defaulting to 1.
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

    pub fn build_constraints(
        &self,
        mode_to_state: &CategoricalStateMapping,
    ) -> Result<Vec<Constraint>, ConstraintModelError> {
        log::debug!(
            "query provided the following modal constraints: {}",
            self.constraints
                .as_ref()
                .map(|cs| cs.iter().map(|c| c.to_string()).join("; "))
                .unwrap_or_else(|| "NONE!".to_string())
        );

        let configs = self.constraints.as_deref().unwrap_or(&[]);
        self.validate_configs(configs, mode_to_state)?;

        let constraints = configs
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

    fn validate_configs(
        &self,
        configs: &[ConstraintConfig],
        mode_to_state: &CategoricalStateMapping,
    ) -> Result<(), ConstraintModelError> {
        let max_legs = self.resolved_max_trip_legs().get() as usize;

        for config in configs {
            match config {
                ConstraintConfig::AllowedModes { values } => {
                    for mode in values {
                        validate_mode_exists(mode, mode_to_state)?;
                    }
                }
                ConstraintConfig::ModeCounts { values } => {
                    for mode in values.keys() {
                        validate_mode_exists(mode, mode_to_state)?;
                    }
                }
                ConstraintConfig::ExactSequences { values } => {
                    for seq in values {
                        if seq.len() > max_legs {
                            return Err(ConstraintModelError::BuildError(format!(
                                "exact_sequences entry {:?} has {} legs, which exceeds max_trip_legs ({})",
                                seq,
                                seq.len(),
                                max_legs
                            )));
                        }
                        for mode in seq {
                            validate_mode_exists(mode, mode_to_state)?;
                        }
                    }
                }
                ConstraintConfig::DistanceConstraint(c) => {
                    validate_distance_bounds(c)?;
                }
                ConstraintConfig::TimeConstraint(c) => {
                    validate_time_bounds(c)?;
                }
                ConstraintConfig::ModeDistanceLimit(values) => {
                    for (mode, c) in values.iter() {
                        validate_mode_exists(mode, mode_to_state)?;
                        validate_distance_bounds(c)?;
                    }
                }
                ConstraintConfig::ModeTimeLimit(values) => {
                    for (mode, c) in values.iter() {
                        validate_mode_exists(mode, mode_to_state)?;
                        validate_time_bounds(c)?;
                    }
                }
                ConstraintConfig::ModeLegDistanceLimit(values) => {
                    for (mode, ModeLegDistanceConstraint { leg, constraint }) in values.iter() {
                        validate_mode_exists(mode, mode_to_state)?;
                        validate_leg_index(leg, max_legs)?;
                        validate_distance_bounds(constraint)?;
                    }
                }
                ConstraintConfig::ModeLegTimeLimit(values) => {
                    for (mode, ModeLegTimeConstraint { leg, constraint }) in values.iter() {
                        validate_mode_exists(mode, mode_to_state)?;
                        validate_leg_index(leg, max_legs)?;
                        validate_time_bounds(constraint)?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn validate_mode_exists(
    mode: &str,
    mode_to_state: &CategoricalStateMapping,
) -> Result<(), ConstraintModelError> {
    if mode_to_state.get_label(mode).is_none() {
        let available = mode_to_state.get_categories().iter().join(", ");
        return Err(ConstraintModelError::BuildError(format!(
            "constraint references unknown travel mode '{mode}' (available modes: [{available}])"
        )));
    }
    Ok(())
}

fn validate_leg_index(
    leg: &TripLegConstraint,
    max_legs: usize,
) -> Result<(), ConstraintModelError> {
    if let TripLegConstraint::LegIndex { index } = leg {
        if *index >= max_legs {
            return Err(ConstraintModelError::BuildError(format!(
                "leg index {index} is out of bounds for max_trip_legs ({max_legs})"
            )));
        }
    }
    Ok(())
}

fn validate_distance_bounds(c: &DistanceConstraint) -> Result<(), ConstraintModelError> {
    if let (Some(min), Some(max)) = (c.min, c.max) {
        if min > max {
            return Err(ConstraintModelError::BuildError(format!(
                "distance constraint has min ({} {}) > max ({} {})",
                c.unit.from_uom(min),
                c.unit,
                c.unit.from_uom(max),
                c.unit
            )));
        }
    }
    Ok(())
}

fn validate_time_bounds(c: &TimeConstraint) -> Result<(), ConstraintModelError> {
    if let (Some(min), Some(max)) = (c.min, c.max) {
        if min > max {
            return Err(ConstraintModelError::BuildError(format!(
                "time constraint has min ({} {}) > max ({} {})",
                c.unit.from_uom(min),
                c.unit,
                c.unit.from_uom(max),
                c.unit
            )));
        }
    }
    Ok(())
}
