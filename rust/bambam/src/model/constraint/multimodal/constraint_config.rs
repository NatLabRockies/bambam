use bambam_core::model::{
    destination::DestinationPredicate, state::multimodal_state_ops as state_ops,
};
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    state::{StateModel, StateVariable},
    unit::*,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroU64;
use uom::si::f64::{Energy, Length, Time};

/// Configuration types for constraining bambam multimodal search.
///
/// Defines various constraint options that can be applied to route searches,
/// including mode restrictions, trip leg constraints, and resource limits, which
/// can be combined into multiple constraints describing a traversal behavior.
///
/// # Examples
///
/// ## Walk mode should not be used for more than a half of a mile, total
///
/// ```toml
/// mode_distance_limit.walk = { limit = 0.5, unit = "miles" }
/// ```
///
/// ## Walk mode can only be used at the beginning and end of a trip
///
/// ```toml
/// mode_sequences = [["walk", "bike", "walk"], ["walk", "drive", "walk"]]
/// ```
///
/// ### Walk mode should not exceed 5m on first leg of trip and 20m total
///
/// ```toml
/// [[constraints]]
/// mode_leg_time_limit.walk = { leg = "first", constraint = { limit = 5.0, unit = "minutes" } }
/// [[constraints]]
/// mode_time_limit.walk = { limit = 20.0, unit = "minutes" }
/// ```
///
/// ### Drive mode legs should never be shorter than 5 minutes or 0.33 miles
///
/// ```toml
/// [[constraints]]
/// mode_time_limit.drive = { limit = 5.0, unit = "minutes", op = "min_exclusive" }
/// [[constraints]]
/// mode_distance_limit.drive = { limit = 0.33, unit = "miles", op = "min_exclusive" }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ConstraintConfig {
    /// Restrict routes to only use allowed transportation modes.
    AllowedModes { allowed_modes: Vec<String> },
    /// Limit the number of times each mode can be used in a route.
    ModeCounts {
        mode_counts: HashMap<String, NonZeroU64>,
    },
    /// Require routes to follow one of the specified mode sequences.
    ExactSequences { exact_sequences: Vec<Vec<String>> },
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

/// Pairs a trip leg constraint with a distance constraint.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModeLegDistanceConstraint {
    pub leg: TripLegConstraint,
    pub constraint: DistanceConstraint,
}

/// Pairs a trip leg constraint with a time constraint.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModeLegTimeConstraint {
    pub leg: TripLegConstraint,
    pub constraint: TimeConstraint,
}

/// Pairs a trip leg constraint with an energy constraint.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModeLegEnergyConstraint {
    pub leg: TripLegConstraint,
    pub constraint: EnergyConstraint,
}

/// operation to use when testing a constraint
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitOperation {
    /// value is greater than or equal to the limit
    MinInclusive,
    /// value is greater than the limit
    MinExclusive,
    /// value is less than or equal to the limit
    #[default]
    MaxInclusive,
    /// value is less than the limit
    MaxExclusive,
}

/// Distance constraint value with associated unit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistanceConstraint {
    pub limit: Length,
    pub unit: DistanceUnit,
    #[serde(default)]
    pub op: LimitOperation,
}

/// Time constraint value with associated unit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimeConstraint {
    pub limit: Time,
    pub unit: TimeUnit,
    #[serde(default)]
    pub op: LimitOperation,
}

/// Energy constraint value with associated unit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnergyConstraint {
    pub limit: Energy,
    pub unit: EnergyUnit,
    pub variable: EnergyStateVariable,
    #[serde(default)]
    pub op: LimitOperation,
}

/// where to grab energy values when comparing against this constraint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EnergyStateVariable {
    /// use the RouteE Compass fieldname for liquid energy types
    Liquid,
    /// use the RouteE Compass fieldname for electric energy types
    Electric,
    /// use both liquid and energy fields and sum the result
    Both,
}

/// Identifies a specific trip leg for constraint application.
///
/// Allows constraints to target specific steps in a trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TripLegConstraint {
    /// the departing trip leg, an alias for LegIndex(0)
    First,
    /// any leg index, starting from zero
    LegIndex { index: usize },
    /// the last possible trip leg index as configured for this search
    Last,
    /// accepts the current trip leg, when the provided destination
    /// predicate is true for this leg/edge combination.
    Arrival {
        destination_predicate: DestinationPredicate,
    },
    /// all legs must meet this criteria independently
    Any,
}

impl LimitOperation {
    /// tests if a given value is within some limit. if it is a min comparison,
    ///
    /// ### mode_switch
    ///
    /// a min* comparison must reserve rejecting edges until it is clear that the trip leg
    /// is ending due to a mode switch. if we have been in leg 0 with mode A, and entering
    /// this edge would move us to leg 1 and mode B, then `mode_switch == true`. at this
    /// point, we can run the limit comparison and decide if such a mode transition is valid
    /// for this trip leg constraint's min limit operation.
    pub fn test<D, U, V>(
        &self,
        value: uom::si::Quantity<D, U, V>,
        limit: uom::si::Quantity<D, U, V>,
        mode_switch: bool,
    ) -> bool
    where
        D: uom::si::Dimension + ?Sized,
        U: uom::si::Units<V> + ?Sized,
        V: uom::num_traits::Num + uom::Conversion<V> + PartialOrd,
    {
        match self {
            LimitOperation::MinInclusive => !mode_switch || value >= limit,
            LimitOperation::MinExclusive => !mode_switch || value > limit,
            LimitOperation::MaxInclusive => value <= limit,
            LimitOperation::MaxExclusive => value < limit,
        }
    }
}

impl DistanceConstraint {
    pub fn test(&self, value: Length, mode_switch: bool) -> bool {
        self.op.test(value, self.limit, mode_switch)
    }
}

impl TimeConstraint {
    pub fn test(&self, value: Time, mode_switch: bool) -> bool {
        self.op.test(value, self.limit, mode_switch)
    }
}

impl EnergyConstraint {
    pub fn test(&self, value: Energy, mode_switch: bool) -> bool {
        self.op.test(value, self.limit, mode_switch)
    }
}

impl TripLegConstraint {
    /// true if the given state vector is a match to the configuration of this TripLegConstraint.
    pub fn matches(
        &self,
        state: &[StateVariable],
        state_model: &StateModel,
        max_trip_legs: NonZeroU64,
    ) -> Result<bool, ConstraintModelError> {
        match self {
            TripLegConstraint::First => matches_leg(state, state_model, 0),
            TripLegConstraint::LegIndex { index } => matches_leg(state, state_model, *index as u64),
            TripLegConstraint::Last => {
                let max_trip_idx = max_trip_legs.get() - 1;
                matches_leg(state, state_model, max_trip_idx)
            }
            TripLegConstraint::Arrival {
                destination_predicate,
            } => destination_predicate
                .valid_destination(state, state_model)
                .map_err(|e| {
                    let msg = format!("while checking trip leg constraint: {e}");
                    ConstraintModelError::ConstraintModelError(msg)
                }),
            TripLegConstraint::Any => Ok(true),
        }
    }
}

/// helper function to test matching of leg index
fn matches_leg(
    state: &[StateVariable],
    state_model: &StateModel,
    leg_idx: u64,
) -> Result<bool, ConstraintModelError> {
    match state_ops::get_active_leg_idx(state, state_model) {
        Ok(None) => Ok(false),
        Ok(Some(idx)) => Ok(idx == leg_idx),
        Err(e) => {
            let msg = format!("while checking trip leg constraint: {e}");
            Err(ConstraintModelError::ConstraintModelError(msg))
        }
    }
}
