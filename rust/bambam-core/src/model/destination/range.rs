use itertools::Itertools;
use routee_compass_core::model::{
    state::{CustomVariableType, StateModel, StateVariable},
    unit::{DistanceUnit, EnergyUnit, TimeUnit},
};
use serde::{Deserialize, Serialize};

use crate::model::destination::DestinationError;

/// type, unit and feature name of the state variable used for binning
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BinRangeConfig {
    Distance {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<u64>,
        /// unit of values
        unit: DistanceUnit,
    },
    Time {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<u64>,
        /// unit of values
        unit: TimeUnit,
    },
    Energy {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<u64>,
        /// unit of values
        unit: EnergyUnit,
    },
    CustomRange {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<u64>,
        /// unit of values
        unit: CustomVariableType,
    },
}

/// a single bin between for values in the range [min, max).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BinRange {
    Distance {
        feature: String,
        min: uom::si::f64::Length,
        max: uom::si::f64::Length,
        unit: DistanceUnit,
    },
    Time {
        feature: String,
        min: uom::si::f64::Time,
        max: uom::si::f64::Time,
        unit: TimeUnit,
    },
    Energy {
        feature: String,
        min: uom::si::f64::Energy,
        max: uom::si::f64::Energy,
        unit: EnergyUnit,
    },
    CustomRange {
        feature: String,
        min: f64,
        max: f64,
        unit: CustomVariableType,
    },
}

impl BinRangeConfig {
    /// create the collection of bins from this configuration. each of these bins will capture
    /// a subset of the destinations.
    pub fn build_bins(&self) -> Vec<BinRange> {
        match self {
            BinRangeConfig::Distance {
                feature,
                values,
                unit,
            } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinRange::Distance {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min as f64),
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinRangeConfig::Time {
                feature,
                values,
                unit,
            } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinRange::Time {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min as f64),
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinRangeConfig::Energy {
                feature,
                values,
                unit,
            } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinRange::Energy {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min as f64),
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinRangeConfig::CustomRange {
                feature,
                values,
                unit,
            } => {
                if matches!(unit, CustomVariableType::Boolean) {
                    let msg = format!("ranged bin for feature '{feature}' is a boolean, but boolean cardinality is 2, which is not large enough to support bins. this may produce undefined behavior.");
                    log::warn!("{}", msg);
                }
                values
                    .iter()
                    .tuple_windows()
                    .map(|(min, max)| BinRange::CustomRange {
                        min: *min as f64,
                        max: *max as f64,
                        feature: feature.to_string(),
                        unit: unit.clone(),
                    })
                    .collect_vec()
            }
        }
    }
}

impl BinRange {
    /// determine whether a trip state is within some bin
    pub fn within_bin(
        &self,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, DestinationError> {
        match self {
            BinRange::Distance {
                feature, min, max, ..
            } => {
                let value = state_model.get_distance(state, feature).map_err(|e| {
                    DestinationError::StateErrorInBin {
                        bin: self.clone(),
                        error: e,
                    }
                })?;
                let result = within_bin(min, &value, max);
                Ok(result)
            }
            BinRange::Time {
                feature, min, max, ..
            } => {
                let value = state_model.get_time(state, feature).map_err(|e| {
                    DestinationError::StateErrorInBin {
                        bin: self.clone(),
                        error: e,
                    }
                })?;
                let result = within_bin(min, &value, max);
                Ok(result)
            }
            BinRange::Energy {
                feature, min, max, ..
            } => {
                let value = state_model.get_energy(state, feature).map_err(|e| {
                    DestinationError::StateErrorInBin {
                        bin: self.clone(),
                        error: e,
                    }
                })?;
                let result = within_bin(min, &value, max);
                Ok(result)
            }
            BinRange::CustomRange {
                feature,
                unit,
                min,
                max,
            } => match unit {
                CustomVariableType::FloatingPoint => {
                    let value = state_model.get_custom_f64(state, feature).map_err(|e| {
                        DestinationError::StateErrorInBin {
                            bin: self.clone(),
                            error: e,
                        }
                    })?;
                    Ok(within_bin(min, &value, max))
                }
                CustomVariableType::SignedInteger => {
                    let value = state_model.get_custom_i64(state, feature).map_err(|e| {
                        DestinationError::StateErrorInBin {
                            bin: self.clone(),
                            error: e,
                        }
                    })?;
                    Ok(within_bin(min, &(value as f64), max))
                }
                CustomVariableType::UnsignedInteger => {
                    let value = state_model.get_custom_u64(state, feature).map_err(|e| {
                        DestinationError::StateErrorInBin {
                            bin: self.clone(),
                            error: e,
                        }
                    })?;
                    Ok(within_bin(min, &(value as f64), max))
                }
                CustomVariableType::Boolean => {
                    let value = state_model.get_custom_bool(state, feature).map_err(|e| {
                        DestinationError::StateErrorInBin {
                            bin: self.clone(),
                            error: e,
                        }
                    })?;
                    Ok(within_bin(min, &((value as i64) as f64), max))
                }
            },
        }
    }
}

impl std::fmt::Display for BinRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinRange::Distance {
                feature,
                min,
                max,
                unit,
            } => {
                let (min_v, max_v) = (unit.from_uom(*min), unit.from_uom(*max));
                format!("{feature} in [{min_v}, {max_v}) {unit}")
            }
            BinRange::Time {
                feature,
                min,
                max,
                unit,
            } => {
                let (min_v, max_v) = (unit.from_uom(*min), unit.from_uom(*max));
                format!("{feature} in [{min_v}, {max_v}) {unit}")
            }
            BinRange::Energy {
                feature,
                min,
                max,
                unit,
            } => {
                let (min_v, max_v) = (unit.from_uom(*min), unit.from_uom(*max));
                format!("{feature} in [{min_v}, {max_v}) {unit}")
            }
            BinRange::CustomRange {
                feature,
                unit,
                min,
                max,
            } => format!("{feature} in [{min}, {max}) stored as {unit}"),
        };
        write!(f, "{s}")
    }
}

/// test for bin membership. bins are defined as lower bound inclusive and
/// upper bound exclusive.
fn within_bin<A, B>(min: &A, val: &B, max: &A) -> bool
where
    A: PartialOrd<B>,
    B: PartialOrd<A>,
{
    min <= val && val < max
}
