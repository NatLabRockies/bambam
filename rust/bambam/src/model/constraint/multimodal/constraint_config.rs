use bambam_core::model::destination::DestinationPredicate;
use routee_compass_core::model::unit::*;
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
/// ## Drive mode trips should not be shorter than 5 minutes
///
/// ```json
/// {
///     "mode_leg_time_limit": {
///         "drive": {
///             "leg": "all",
///             "constraint": { "limit": 5.0, "unit": "minutes" }
///         }
///     }
/// }
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
        mode_leg_distance_limit: HashMap<String, ModeLegTimeConstraint>,
    },
    /// Set energy limits for specific modes on specific trip legs.
    ModeLegEnergyLimit {
        mode_leg_distance_limit: HashMap<String, ModeLegEnergyConstraint>,
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
    /// value is less than or equal to the limit
    #[default]
    MaxInclusive,
    /// value is less than the limit
    MaxExclusive,
}

/// Distance constraint value with associated unit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DistanceConstraint {
    limit: f64,
    #[serde(default)]
    op: LimitOperation,
    unit: DistanceUnit,
}

/// Time constraint value with associated unit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimeConstraint {
    limit: f64,
    #[serde(default)]
    op: LimitOperation,
    unit: TimeUnit,
}

/// Energy constraint value with associated unit.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnergyConstraint {
    limit: f64,
    #[serde(default)]
    op: LimitOperation,
    unit: EnergyUnit,
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
    /// any arrival trip leg, when the provided destination
    /// predicate is true.
    Arrival {
        destination_predicate: DestinationPredicate,
    },
    /// all legs must meet this criteria independently
    Any,
}

impl LimitOperation {
    /// tests if a given value is within some limit
    pub fn test(&self, value: f64, limit: f64) -> bool {
        match self {
            LimitOperation::MaxInclusive => value <= limit,
            LimitOperation::MaxExclusive => value < limit,
        }
    }
}

impl DistanceConstraint {
    pub fn test(&self, value: Length) -> bool {
        let value_f64 = self.unit.from_uom(value);
        self.op.test(value_f64, self.limit)
    }
}

impl TimeConstraint {
    pub fn test(&self, value: Time) -> bool {
        let value_f64 = self.unit.from_uom(value);
        self.op.test(value_f64, self.limit)
    }
}

impl EnergyConstraint {
    pub fn test(&self, value: Energy) -> bool {
        let value_f64 = self.unit.from_uom(value);
        self.op.test(value_f64, self.limit)
    }
}
