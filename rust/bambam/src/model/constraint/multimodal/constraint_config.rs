use bambam_core::model::{
    destination::DestinationPredicate, state::multimodal_state_ops as state_ops,
};
use itertools::Itertools;
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
/// type = "mode_distance_limit"
/// values.walk = { limit = 0.5, unit = "miles" }
/// ```
///
/// ## Walk mode can only be used in the first or third leg of a trip
/// which may include a middle leg in either bike or drive mode.
///
/// ```toml
/// type = "exact_sequences"
/// values = [["walk", "bike", "walk"], ["walk", "drive", "walk"]]
/// ```
///
/// ### Walk mode should not exceed 5m on first leg of trip and 20m total
///
/// ```toml
/// [[constraints]]
/// type = "mode_leg_time_limit"
/// values.walk = { leg.type = "first", constraint = { limit = 5.0, unit = "minutes" } }
/// [[constraints]]
/// type = "mode_time_limit"
/// values.walk = { limit = 20.0, unit = "minutes" }
/// ```
///
/// ### Drive mode legs should never be shorter than 5 minutes or 0.33 miles
///
/// ```toml
/// [[constraints]]
/// type = "mode_time_limit"
/// values.drive = { limit = 5.0, unit = "minutes", op = "min_exclusive" }
/// [[constraints]]
/// type = "mode_distance_limit"
/// values.drive = { limit = 0.33, unit = "miles", op = "min_exclusive" }
/// ```
///
/// ### Drive mode should not exceed 2 gallons of gas
///
/// ```toml
/// [[constraints]]
/// type = "mode_energy_limit"
/// values.drive = { limit = 2.0, unit = "gallons_gasoline_equivalent", variable = "liquid" }
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstraintConfig {
    /// Restrict routes to only use allowed transportation modes.
    AllowedModes { values: Vec<String> },
    /// Limit the number of times each mode can be used in a route.
    ModeCounts { values: HashMap<String, NonZeroU64> },
    /// Require routes to follow one of the specified mode sequences.
    ExactSequences { values: Vec<Vec<String>> },
    /// Set distance limit for the trip.
    #[serde(rename = "distance_limit")]
    DistanceConstraint(DistanceConstraint),
    /// Set time limit for the trip.
    #[serde(rename = "time_limit")]
    TimeConstraint(TimeConstraint),
    // /// Set maximum time limit for the trip.
    // #[serde(rename = "energy_limit")]
    // EnergyConstraint,
    /// Set maximum distance limits for each transportation mode.
    ModeDistanceLimit {
        values: HashMap<String, DistanceConstraint>,
    },
    /// Set maximum time limits for each transportation mode.
    ModeTimeLimit {
        values: HashMap<String, TimeConstraint>,
    },
    // /// Set maximum energy limits for each transportation mode.
    // ModeEnergyLimit {
    //     values: HashMap<String, EnergyConstraint>,
    // },
    /// Set distance limits for specific modes on specific trip legs.
    ModeLegDistanceLimit {
        values: HashMap<String, ModeLegDistanceConstraint>,
    },
    /// Set time limits for specific modes on specific trip legs.
    ModeLegTimeLimit {
        values: HashMap<String, ModeLegTimeConstraint>,
    },
    // /// Set energy limits for specific modes on specific trip legs.
    // ModeLegEnergyLimit {
    //     values: HashMap<String, ModeLegEnergyConstraint>,
    // },
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
#[derive(Serialize, Debug, Clone)]
pub struct DistanceConstraint {
    pub limit: Length,
    pub unit: DistanceUnit,
    #[serde(default)]
    pub op: LimitOperation,
}

/// Time constraint value with associated unit.
#[derive(Serialize, Debug, Clone)]
pub struct TimeConstraint {
    pub limit: Time,
    pub unit: TimeUnit,
    #[serde(default)]
    pub op: LimitOperation,
}

/// Energy constraint value with associated unit.
#[derive(Serialize, Debug, Clone)]
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
                // safe to call -1 here on max_trip_legs which is strictly > 0
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

impl<'de> Deserialize<'de> for DistanceConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawConstraint {
            limit: f64,
            unit: DistanceUnit,
            #[serde(default)]
            op: LimitOperation,
        }

        let raw = RawConstraint::deserialize(deserializer)?;
        let limit = raw.unit.to_uom(raw.limit);
        Ok(DistanceConstraint {
            limit,
            unit: raw.unit,
            op: raw.op,
        })
    }
}

impl<'de> Deserialize<'de> for TimeConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawConstraint {
            limit: f64,
            unit: TimeUnit,
            #[serde(default)]
            op: LimitOperation,
        }

        let raw = RawConstraint::deserialize(deserializer)?;
        let limit = raw.unit.to_uom(raw.limit);
        Ok(TimeConstraint {
            limit,
            unit: raw.unit,
            op: raw.op,
        })
    }
}

impl<'de> Deserialize<'de> for EnergyConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawConstraint {
            limit: f64,
            unit: EnergyUnit,
            variable: EnergyStateVariable,
            #[serde(default)]
            op: LimitOperation,
        }

        let raw = RawConstraint::deserialize(deserializer)?;
        let limit = raw.unit.to_uom(raw.limit);
        Ok(EnergyConstraint {
            limit,
            unit: raw.unit,
            variable: raw.variable,
            op: raw.op,
        })
    }
}

impl std::fmt::Display for LimitOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LimitOperation::MinInclusive => "min-inclusive",
            LimitOperation::MinExclusive => "mix-exclusive",
            LimitOperation::MaxInclusive => "max-inclusive",
            LimitOperation::MaxExclusive => "max-exclusive",
        };
        write!(f, "{s}")
    }
}

impl std::fmt::Display for TripLegConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TripLegConstraint::First => write!(f, "first leg"),
            TripLegConstraint::LegIndex { index } => write!(f, "{index}'th leg index"),
            TripLegConstraint::Last => write!(f, "last leg"),
            TripLegConstraint::Arrival {
                destination_predicate: p,
            } => write!(f, "{p}"),
            TripLegConstraint::Any => write!(f, "any leg"),
        }
    }
}

impl std::fmt::Display for DistanceConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "distance constraint of {} {} ({})",
            self.unit.from_uom(self.limit),
            self.unit,
            self.op
        )
    }
}

impl std::fmt::Display for TimeConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "time constraint of {} {} ({})",
            self.unit.from_uom(self.limit),
            self.unit,
            self.op
        )
    }
}

impl std::fmt::Display for ModeLegDistanceConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.leg, self.constraint)
    }
}

impl std::fmt::Display for ModeLegTimeConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.leg, self.constraint)
    }
}

impl std::fmt::Display for ConstraintConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConstraintConfig::AllowedModes { values } => {
                write!(f, "{}", pretty_list(values))
            }
            ConstraintConfig::ModeCounts { values } => write!(f, "{}", pretty_map(values)),
            ConstraintConfig::ExactSequences { values } => {
                let sequence_strings = values.iter().map(|path| path.join("->")).collect_vec();
                write!(f, "{}", pretty_list(&sequence_strings))
            }
            ConstraintConfig::DistanceConstraint(constraint) => write!(f, "overall {}", constraint),
            ConstraintConfig::TimeConstraint(constraint) => write!(f, "overall {}", constraint),
            ConstraintConfig::ModeDistanceLimit { values } => write!(f, "{}", pretty_map(values)),
            ConstraintConfig::ModeTimeLimit { values } => write!(f, "{}", pretty_map(values)),
            ConstraintConfig::ModeLegDistanceLimit { values } => {
                write!(f, "{}", pretty_map(values))
            }
            ConstraintConfig::ModeLegTimeLimit { values } => write!(f, "{}", pretty_map(values)),
        }
    }
}

/// helper function to pretty print a list of string values using English grammar conventions for ",", "and".
fn pretty_list(values: &[String]) -> String {
    match values.iter().as_slice() {
        [] => String::from("no travel modes allowed!"),
        [single] => format!("allow travel by {single} mode"),
        [first, second] => {
            format!("allow travel by {first} and {second} modes")
        }
        [.., last] => {
            let most_str = values.iter().dropping(1).join(", ");
            format!("allow travel by {most_str} and {last} modes")
        }
        _ => unreachable!(),
    }
}

/// helper function to pretty print a hashmap where the key is a modename and the value is
/// a [std::fmt::Display]-able metric constraint.
fn pretty_map<T>(values: &HashMap<String, T>) -> String
where
    T: std::fmt::Display,
{
    values
        .iter()
        .map(|(k, v)| format!("mode {k} has a {v}"))
        .join(", ")
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
