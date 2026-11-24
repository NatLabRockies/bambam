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
/// mode = "walk"
/// max = 0.5
/// unit = "miles"
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
/// ### Walk mode should not exceed 5m on first leg of trip and 20m total (inline shorthand)
///
/// ```toml
/// constraints = [
///     { type = "mode_leg_time_limit", mode = "walk", leg = "first", max = 5.0, unit = "minutes" },
///     { type = "mode_time_limit", mode = "walk", max = 20.0, unit = "minutes" },
/// ]
/// ```
///
/// ### Multi-mode map syntax is also supported
///
/// ```toml
/// [[constraints]]
/// type = "mode_leg_time_limit"
/// values.walk = { leg = "first", max = 5.0, unit = "minutes" }
/// values.drive = { min = 5.0, max = 35.0, unit = "minutes" }
/// ```
///
/// ### Drive mode legs should never be shorter than 5 minutes or 0.33 miles
///
/// ```toml
/// [[constraints]]
/// type = "mode_leg_time_limit"
/// mode = "drive"
/// min = 5.0
/// unit = "minutes"
/// [[constraints]]
/// type = "mode_leg_distance_limit"
/// mode = "drive"
/// min = 0.33
/// unit = "miles"
/// ```
///
/// ### Drive mode should not exceed 2 gallons of gas
///
/// ```toml
/// [[constraints]]
/// type = "mode_energy_limit"
/// mode = "drive"
/// max = 2.0
/// unit = "gallons_gasoline_equivalent"
/// variable = "liquid"
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
    /// Set distance limits for each transportation mode.
    ModeDistanceLimit(ModeConstraintMap<DistanceConstraint>),
    /// Set time limits for each transportation mode.
    ModeTimeLimit(ModeConstraintMap<TimeConstraint>),
    // /// Set maximum energy limits for each transportation mode.
    // ModeEnergyLimit {
    //     values: HashMap<String, EnergyConstraint>,
    // },
    /// Set distance limits for specific modes on specific trip legs.
    ModeLegDistanceLimit(ModeConstraintMap<ModeLegDistanceConstraint>),
    /// Set time limits for specific modes on specific trip legs.
    ModeLegTimeLimit(ModeConstraintMap<ModeLegTimeConstraint>),
    // /// Set energy limits for specific modes on specific trip legs.
    // ModeLegEnergyLimit {
    //     values: HashMap<String, ModeLegEnergyConstraint>,
    // },
}

/// Helper wrapper allowing mode-keyed constraint configs to be written either as a map
/// (`values = { walk = { ... } }`) or inline for a single mode (`mode = "walk", ...`).
#[derive(Debug, Clone)]
pub struct ModeConstraintMap<T>(pub HashMap<String, T>);

impl<T> Serialize for ModeConstraintMap<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct MapWrapper<'a, T> {
            values: &'a HashMap<String, T>,
        }
        MapWrapper { values: &self.0 }.serialize(serializer)
    }
}

impl<T> std::ops::Deref for ModeConstraintMap<T> {
    type Target = HashMap<String, T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de, T> Deserialize<'de> for ModeConstraintMap<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawMapOrSingle<T> {
            Map {
                values: HashMap<String, T>,
            },
            Single {
                mode: String,
                #[serde(flatten)]
                constraint: T,
            },
        }

        match RawMapOrSingle::<T>::deserialize(deserializer)? {
            RawMapOrSingle::Map { values } => Ok(ModeConstraintMap(values)),
            RawMapOrSingle::Single { mode, constraint } => {
                Ok(ModeConstraintMap(HashMap::from([(mode, constraint)])))
            }
        }
    }
}

/// Pairs a trip leg constraint with a distance constraint.
/// Supports both flattened syntax (`{ leg = ..., min = ..., max = ..., unit = ... }`)
/// and nested syntax (`{ leg = ..., constraint = { ... } }`).
#[derive(Serialize, Debug, Clone)]
pub struct ModeLegDistanceConstraint {
    #[serde(default)]
    pub leg: TripLegConstraint,
    #[serde(flatten)]
    pub constraint: DistanceConstraint,
}

/// Pairs a trip leg constraint with a time constraint.
/// Supports both flattened syntax (`{ leg = ..., min = ..., max = ..., unit = ... }`)
/// and nested syntax (`{ leg = ..., constraint = { ... } }`).
#[derive(Serialize, Debug, Clone)]
pub struct ModeLegTimeConstraint {
    #[serde(default)]
    pub leg: TripLegConstraint,
    #[serde(flatten)]
    pub constraint: TimeConstraint,
}

/// Pairs a trip leg constraint with an energy constraint.
#[derive(Serialize, Debug, Clone)]
pub struct ModeLegEnergyConstraint {
    #[serde(default)]
    pub leg: TripLegConstraint,
    #[serde(flatten)]
    pub constraint: EnergyConstraint,
}

/// operation to use when testing a legacy single-bound constraint
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// Distance constraint with optional `min` and/or `max` bounds and associated unit.
#[derive(Debug, Clone)]
pub struct DistanceConstraint {
    pub min: Option<Length>,
    pub max: Option<Length>,
    pub unit: DistanceUnit,
    pub min_exclusive: bool,
    pub max_exclusive: bool,
}

/// Time constraint with optional `min` and/or `max` bounds and associated unit.
#[derive(Debug, Clone)]
pub struct TimeConstraint {
    pub min: Option<Time>,
    pub max: Option<Time>,
    pub unit: TimeUnit,
    pub min_exclusive: bool,
    pub max_exclusive: bool,
}

/// Energy constraint with optional `min` and/or `max` bounds and associated unit.
#[derive(Debug, Clone)]
pub struct EnergyConstraint {
    pub min: Option<Energy>,
    pub max: Option<Energy>,
    pub unit: EnergyUnit,
    pub variable: EnergyStateVariable,
    pub min_exclusive: bool,
    pub max_exclusive: bool,
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
/// Supports both string/integer shorthands (`"first"`, `"last"`, `"any"`, `0`)
/// and tagged object syntax (`{ type = "first" }`, `{ type = "leg_index", index = 0 }`, etc.).
#[derive(Debug, Clone, Default, Serialize)]
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
    #[default]
    Any,
}

impl<'de> Deserialize<'de> for TripLegConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum TaggedLeg {
            First,
            LegIndex {
                index: usize,
            },
            Last,
            Arrival {
                destination_predicate: DestinationPredicate,
            },
            Any,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawLeg {
            Str(String),
            Index(usize),
            Tagged(TaggedLeg),
        }

        match RawLeg::deserialize(deserializer)? {
            RawLeg::Str(s) => match s.as_str() {
                "first" => Ok(TripLegConstraint::First),
                "last" => Ok(TripLegConstraint::Last),
                "any" => Ok(TripLegConstraint::Any),
                other => Err(serde::de::Error::custom(format!(
                    "unknown leg shorthand '{other}', expected 'first', 'last', 'any', a leg index integer, or a tagged object"
                ))),
            },
            RawLeg::Index(index) => Ok(TripLegConstraint::LegIndex { index }),
            RawLeg::Tagged(t) => Ok(match t {
                TaggedLeg::First => TripLegConstraint::First,
                TaggedLeg::LegIndex { index } => TripLegConstraint::LegIndex { index },
                TaggedLeg::Last => TripLegConstraint::Last,
                TaggedLeg::Arrival {
                    destination_predicate,
                } => TripLegConstraint::Arrival {
                    destination_predicate,
                },
                TaggedLeg::Any => TripLegConstraint::Any,
            }),
        }
    }
}

impl LimitOperation {
    pub fn is_min(&self) -> bool {
        matches!(
            self,
            LimitOperation::MinInclusive | LimitOperation::MinExclusive
        )
    }

    pub fn is_max(&self) -> bool {
        matches!(
            self,
            LimitOperation::MaxInclusive | LimitOperation::MaxExclusive
        )
    }

    /// tests if a given value is within some limit.
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
    pub fn new(min: Option<Length>, max: Option<Length>, unit: DistanceUnit) -> Self {
        Self {
            min,
            max,
            unit,
            min_exclusive: false,
            max_exclusive: false,
        }
    }

    pub fn has_min(&self) -> bool {
        self.min.is_some()
    }

    pub fn has_max(&self) -> bool {
        self.max.is_some()
    }

    pub fn test_min(&self, value: Length) -> bool {
        match self.min {
            Some(limit) if self.min_exclusive => value > limit,
            Some(limit) => value >= limit,
            None => true,
        }
    }

    pub fn test_max(&self, value: Length) -> bool {
        match self.max {
            Some(limit) if self.max_exclusive => value < limit,
            Some(limit) => value <= limit,
            None => true,
        }
    }

    pub fn test(&self, value: Length, mode_switch: bool) -> bool {
        let min_ok = !mode_switch || self.test_min(value);
        let max_ok = self.test_max(value);
        min_ok && max_ok
    }
}

impl Serialize for DistanceConstraint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct RawConstraint {
            #[serde(skip_serializing_if = "Option::is_none")]
            min: Option<f64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max: Option<f64>,
            unit: DistanceUnit,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            min_exclusive: bool,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            max_exclusive: bool,
        }
        let raw = RawConstraint {
            min: self.min.map(|v| self.unit.from_uom(v)),
            max: self.max.map(|v| self.unit.from_uom(v)),
            unit: self.unit,
            min_exclusive: self.min_exclusive,
            max_exclusive: self.max_exclusive,
        };
        raw.serialize(serializer)
    }
}

impl TimeConstraint {
    pub fn new(min: Option<Time>, max: Option<Time>, unit: TimeUnit) -> Self {
        Self {
            min,
            max,
            unit,
            min_exclusive: false,
            max_exclusive: false,
        }
    }

    pub fn has_min(&self) -> bool {
        self.min.is_some()
    }

    pub fn has_max(&self) -> bool {
        self.max.is_some()
    }

    pub fn test_min(&self, value: Time) -> bool {
        match self.min {
            Some(limit) if self.min_exclusive => value > limit,
            Some(limit) => value >= limit,
            None => true,
        }
    }

    pub fn test_max(&self, value: Time) -> bool {
        match self.max {
            Some(limit) if self.max_exclusive => value < limit,
            Some(limit) => value <= limit,
            None => true,
        }
    }

    pub fn test(&self, value: Time, mode_switch: bool) -> bool {
        let min_ok = !mode_switch || self.test_min(value);
        let max_ok = self.test_max(value);
        min_ok && max_ok
    }
}

impl Serialize for TimeConstraint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct RawConstraint {
            #[serde(skip_serializing_if = "Option::is_none")]
            min: Option<f64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max: Option<f64>,
            unit: TimeUnit,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            min_exclusive: bool,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            max_exclusive: bool,
        }
        let raw = RawConstraint {
            min: self.min.map(|v| self.unit.from_uom(v)),
            max: self.max.map(|v| self.unit.from_uom(v)),
            unit: self.unit,
            min_exclusive: self.min_exclusive,
            max_exclusive: self.max_exclusive,
        };
        raw.serialize(serializer)
    }
}

impl EnergyConstraint {
    pub fn new(
        min: Option<Energy>,
        max: Option<Energy>,
        unit: EnergyUnit,
        variable: EnergyStateVariable,
    ) -> Self {
        Self {
            min,
            max,
            unit,
            variable,
            min_exclusive: false,
            max_exclusive: false,
        }
    }

    pub fn has_min(&self) -> bool {
        self.min.is_some()
    }

    pub fn has_max(&self) -> bool {
        self.max.is_some()
    }

    pub fn test_min(&self, value: Energy) -> bool {
        match self.min {
            Some(limit) if self.min_exclusive => value > limit,
            Some(limit) => value >= limit,
            None => true,
        }
    }

    pub fn test_max(&self, value: Energy) -> bool {
        match self.max {
            Some(limit) if self.max_exclusive => value < limit,
            Some(limit) => value <= limit,
            None => true,
        }
    }

    pub fn test(&self, value: Energy, mode_switch: bool) -> bool {
        let min_ok = !mode_switch || self.test_min(value);
        let max_ok = self.test_max(value);
        min_ok && max_ok
    }
}

impl Serialize for EnergyConstraint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct RawConstraint {
            #[serde(skip_serializing_if = "Option::is_none")]
            min: Option<f64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max: Option<f64>,
            unit: EnergyUnit,
            variable: EnergyStateVariable,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            min_exclusive: bool,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            max_exclusive: bool,
        }
        let raw = RawConstraint {
            min: self.min.map(|v| self.unit.from_uom(v)),
            max: self.max.map(|v| self.unit.from_uom(v)),
            unit: self.unit,
            variable: self.variable.clone(),
            min_exclusive: self.min_exclusive,
            max_exclusive: self.max_exclusive,
        };
        raw.serialize(serializer)
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
        let active_leg_idx = state_ops::get_active_leg_idx(state, state_model).map_err(|e| {
            let msg = format!("while checking trip leg constraint: {e}");
            ConstraintModelError::ConstraintModelError(msg)
        })?;
        self.matches_leg_idx(active_leg_idx, state, state_model, max_trip_legs)
    }

    /// true if the provided `leg_idx` matches this `TripLegConstraint`.
    pub fn matches_leg_idx(
        &self,
        leg_idx: Option<u64>,
        state: &[StateVariable],
        state_model: &StateModel,
        max_trip_legs: NonZeroU64,
    ) -> Result<bool, ConstraintModelError> {
        match self {
            TripLegConstraint::First => Ok(leg_idx == Some(0)),
            TripLegConstraint::LegIndex { index } => Ok(leg_idx == Some(*index as u64)),
            TripLegConstraint::Last => {
                // safe to call -1 here on max_trip_legs which is strictly > 0
                let max_trip_idx = max_trip_legs.get() - 1;
                Ok(leg_idx == Some(max_trip_idx))
            }
            TripLegConstraint::Arrival {
                destination_predicate,
            } => destination_predicate
                .valid_destination(state, state_model)
                .map_err(|e| {
                    let msg = format!("while checking trip leg constraint: {e}");
                    ConstraintModelError::ConstraintModelError(msg)
                }),
            TripLegConstraint::Any => Ok(leg_idx.is_some()),
        }
    }
}

impl<'de> Deserialize<'de> for DistanceConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawConstraint {
            min: Option<f64>,
            max: Option<f64>,
            limit: Option<f64>,
            unit: DistanceUnit,
            op: Option<LimitOperation>,
            #[serde(default)]
            min_exclusive: bool,
            #[serde(default)]
            max_exclusive: bool,
        }

        let raw = RawConstraint::deserialize(deserializer)?;
        let mut min = raw.min.map(|v| raw.unit.to_uom(v));
        let mut max = raw.max.map(|v| raw.unit.to_uom(v));
        let mut min_exclusive = raw.min_exclusive;
        let mut max_exclusive = raw.max_exclusive;

        if let Some(limit_val) = raw.limit {
            let limit_uom = raw.unit.to_uom(limit_val);
            match raw.op.unwrap_or_default() {
                LimitOperation::MinInclusive => {
                    min = Some(limit_uom);
                    min_exclusive = false;
                }
                LimitOperation::MinExclusive => {
                    min = Some(limit_uom);
                    min_exclusive = true;
                }
                LimitOperation::MaxInclusive => {
                    max = Some(limit_uom);
                    max_exclusive = false;
                }
                LimitOperation::MaxExclusive => {
                    max = Some(limit_uom);
                    max_exclusive = true;
                }
            }
        }

        if min.is_none() && max.is_none() {
            return Err(serde::de::Error::custom(
                "distance constraint requires at least one of 'min', 'max', or 'limit'",
            ));
        }

        Ok(DistanceConstraint {
            min,
            max,
            unit: raw.unit,
            min_exclusive,
            max_exclusive,
        })
    }
}

impl<'de> Deserialize<'de> for TimeConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawConstraint {
            min: Option<f64>,
            max: Option<f64>,
            limit: Option<f64>,
            unit: TimeUnit,
            op: Option<LimitOperation>,
            #[serde(default)]
            min_exclusive: bool,
            #[serde(default)]
            max_exclusive: bool,
        }

        let raw = RawConstraint::deserialize(deserializer)?;
        let mut min = raw.min.map(|v| raw.unit.to_uom(v));
        let mut max = raw.max.map(|v| raw.unit.to_uom(v));
        let mut min_exclusive = raw.min_exclusive;
        let mut max_exclusive = raw.max_exclusive;

        if let Some(limit_val) = raw.limit {
            let limit_uom = raw.unit.to_uom(limit_val);
            match raw.op.unwrap_or_default() {
                LimitOperation::MinInclusive => {
                    min = Some(limit_uom);
                    min_exclusive = false;
                }
                LimitOperation::MinExclusive => {
                    min = Some(limit_uom);
                    min_exclusive = true;
                }
                LimitOperation::MaxInclusive => {
                    max = Some(limit_uom);
                    max_exclusive = false;
                }
                LimitOperation::MaxExclusive => {
                    max = Some(limit_uom);
                    max_exclusive = true;
                }
            }
        }

        if min.is_none() && max.is_none() {
            return Err(serde::de::Error::custom(
                "time constraint requires at least one of 'min', 'max', or 'limit'",
            ));
        }

        Ok(TimeConstraint {
            min,
            max,
            unit: raw.unit,
            min_exclusive,
            max_exclusive,
        })
    }
}

impl<'de> Deserialize<'de> for EnergyConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawConstraint {
            min: Option<f64>,
            max: Option<f64>,
            limit: Option<f64>,
            unit: EnergyUnit,
            variable: EnergyStateVariable,
            op: Option<LimitOperation>,
            #[serde(default)]
            min_exclusive: bool,
            #[serde(default)]
            max_exclusive: bool,
        }

        let raw = RawConstraint::deserialize(deserializer)?;
        let mut min = raw.min.map(|v| raw.unit.to_uom(v));
        let mut max = raw.max.map(|v| raw.unit.to_uom(v));
        let mut min_exclusive = raw.min_exclusive;
        let mut max_exclusive = raw.max_exclusive;

        if let Some(limit_val) = raw.limit {
            let limit_uom = raw.unit.to_uom(limit_val);
            match raw.op.unwrap_or_default() {
                LimitOperation::MinInclusive => {
                    min = Some(limit_uom);
                    min_exclusive = false;
                }
                LimitOperation::MinExclusive => {
                    min = Some(limit_uom);
                    min_exclusive = true;
                }
                LimitOperation::MaxInclusive => {
                    max = Some(limit_uom);
                    max_exclusive = false;
                }
                LimitOperation::MaxExclusive => {
                    max = Some(limit_uom);
                    max_exclusive = true;
                }
            }
        }

        if min.is_none() && max.is_none() {
            return Err(serde::de::Error::custom(
                "energy constraint requires at least one of 'min', 'max', or 'limit'",
            ));
        }

        Ok(EnergyConstraint {
            min,
            max,
            unit: raw.unit,
            variable: raw.variable,
            min_exclusive,
            max_exclusive,
        })
    }
}

impl<'de> Deserialize<'de> for ModeLegDistanceConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawModeLegDistance {
            Nested {
                #[serde(default)]
                leg: TripLegConstraint,
                constraint: DistanceConstraint,
            },
            Flattened {
                #[serde(default)]
                leg: TripLegConstraint,
                #[serde(flatten)]
                constraint: DistanceConstraint,
            },
        }

        match RawModeLegDistance::deserialize(deserializer)? {
            RawModeLegDistance::Nested { leg, constraint }
            | RawModeLegDistance::Flattened { leg, constraint } => {
                Ok(ModeLegDistanceConstraint { leg, constraint })
            }
        }
    }
}

impl<'de> Deserialize<'de> for ModeLegTimeConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawModeLegTime {
            Nested {
                #[serde(default)]
                leg: TripLegConstraint,
                constraint: TimeConstraint,
            },
            Flattened {
                #[serde(default)]
                leg: TripLegConstraint,
                #[serde(flatten)]
                constraint: TimeConstraint,
            },
        }

        match RawModeLegTime::deserialize(deserializer)? {
            RawModeLegTime::Nested { leg, constraint }
            | RawModeLegTime::Flattened { leg, constraint } => {
                Ok(ModeLegTimeConstraint { leg, constraint })
            }
        }
    }
}

impl<'de> Deserialize<'de> for ModeLegEnergyConstraint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawModeLegEnergy {
            Nested {
                #[serde(default)]
                leg: TripLegConstraint,
                constraint: EnergyConstraint,
            },
            Flattened {
                #[serde(default)]
                leg: TripLegConstraint,
                #[serde(flatten)]
                constraint: EnergyConstraint,
            },
        }

        match RawModeLegEnergy::deserialize(deserializer)? {
            RawModeLegEnergy::Nested { leg, constraint }
            | RawModeLegEnergy::Flattened { leg, constraint } => {
                Ok(ModeLegEnergyConstraint { leg, constraint })
            }
        }
    }
}

impl std::fmt::Display for LimitOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LimitOperation::MinInclusive => "min-inclusive",
            LimitOperation::MinExclusive => "min-exclusive",
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
        match (self.min, self.max) {
            (Some(min), Some(max)) => write!(
                f,
                "distance constraint in [{}, {}] {}",
                self.unit.from_uom(min),
                self.unit.from_uom(max),
                self.unit
            ),
            (Some(min), None) => write!(
                f,
                "distance constraint >= {} {}",
                self.unit.from_uom(min),
                self.unit
            ),
            (None, Some(max)) => write!(
                f,
                "distance constraint <= {} {}",
                self.unit.from_uom(max),
                self.unit
            ),
            (None, None) => write!(f, "unbounded distance constraint"),
        }
    }
}

impl std::fmt::Display for TimeConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.min, self.max) {
            (Some(min), Some(max)) => write!(
                f,
                "time constraint in [{}, {}] {}",
                self.unit.from_uom(min),
                self.unit.from_uom(max),
                self.unit
            ),
            (Some(min), None) => write!(
                f,
                "time constraint >= {} {}",
                self.unit.from_uom(min),
                self.unit
            ),
            (None, Some(max)) => write!(
                f,
                "time constraint <= {} {}",
                self.unit.from_uom(max),
                self.unit
            ),
            (None, None) => write!(f, "unbounded time constraint"),
        }
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
            ConstraintConfig::ModeDistanceLimit(values) => write!(f, "{}", pretty_map(values)),
            ConstraintConfig::ModeTimeLimit(values) => write!(f, "{}", pretty_map(values)),
            ConstraintConfig::ModeLegDistanceLimit(values) => {
                write!(f, "{}", pretty_map(values))
            }
            ConstraintConfig::ModeLegTimeLimit(values) => write!(f, "{}", pretty_map(values)),
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
