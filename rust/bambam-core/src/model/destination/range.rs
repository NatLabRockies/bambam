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
        /// if true, injects a leading "0" for the bin values.
        /// it is typical to describe bins by only their max values, which
        /// results in omitting zero.
        #[serde(default = "prepend_zero_default")]
        prepend_zero: bool,
    },
    Time {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<u64>,
        /// unit of values
        unit: TimeUnit,
        /// if true, injects a leading "0" for the bin values.
        /// it is typical to describe bins by only their max values, which
        /// results in omitting zero.
        #[serde(default = "prepend_zero_default")]
        prepend_zero: bool,
    },
    Energy {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<u64>,
        /// unit of values
        unit: EnergyUnit,
        /// if true, injects a leading "0" for the bin values.
        /// it is typical to describe bins by only their max values, which
        /// results in omitting zero.
        #[serde(default = "prepend_zero_default")]
        prepend_zero: bool,
    },
    CustomRange {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<u64>,
        /// unit of values
        unit: CustomVariableType,
        /// if true, injects a leading "0" for the bin values.
        /// it is typical to describe bins by only their max values, which
        /// results in omitting zero.
        #[serde(default = "prepend_zero_default")]
        prepend_zero: bool,
    },
}

fn prepend_zero_default() -> bool {
    true
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
    /// Accessor for the values slice, regardless of variant, as an always-ascending
    /// list of numbers with no duplicates.
    fn values_ascending(&self) -> Vec<u64> {
        let mut values = match self {
            BinRangeConfig::Distance { values, .. }
            | BinRangeConfig::Time { values, .. }
            | BinRangeConfig::Energy { values, .. }
            | BinRangeConfig::CustomRange { values, .. } => values.clone(),
        };
        values.sort();
        values.dedup();
        values
    }

    /// true if the configured binning values should have a zero value prepended.
    fn prepend_zero(&self) -> bool {
        match self {
            BinRangeConfig::Distance { prepend_zero, .. }
            | BinRangeConfig::Time { prepend_zero, .. }
            | BinRangeConfig::Energy { prepend_zero, .. }
            | BinRangeConfig::CustomRange { prepend_zero, .. } => *prepend_zero,
        }
    }

    /// create the collection of bins from this configuration. each of these bins will capture
    /// a subset of the destinations.
    pub fn build_bins(&self) -> Result<Vec<BinRange>, DestinationError> {
        let mut values = self.values_ascending();
        if values.len() < 2 {
            return Err(DestinationError::InvalidBinConfig {
                reason: format!(
                    "bin range config requires at least 2 values to form a bin, got {}",
                    values.len()
                ),
            });
        }
        if self.prepend_zero() {
            values.insert(0, 0);
        }

        let bins = match self {
            BinRangeConfig::Distance { feature, unit, .. } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinRange::Distance {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min as f64),
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinRangeConfig::Time { feature, unit, .. } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinRange::Time {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min as f64),
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinRangeConfig::Energy { feature, unit, .. } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinRange::Energy {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min as f64),
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinRangeConfig::CustomRange { feature, unit, .. } => {
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
        };
        Ok(bins)
    }
}

impl BinRange {
    /// Returns a stable string key for this bin derived from its upper bound in
    /// the configured unit, rounded to the nearest integer.  For a `Time` bin
    /// with max = 10 minutes this returns `"10"`, matching the previous
    /// `TimeBin::key()` convention.
    pub fn bin_key(&self) -> String {
        match self {
            BinRange::Time { max, unit, .. } => {
                format!("{}", unit.from_uom(*max).round() as u64)
            }
            BinRange::Distance { max, unit, .. } => {
                format!("{}", unit.from_uom(*max).round() as u64)
            }
            BinRange::Energy { max, unit, .. } => {
                format!("{}", unit.from_uom(*max).round() as u64)
            }
            BinRange::CustomRange { max, .. } => {
                format!("{}", max.round() as u64)
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use super::BinRangeConfig;
    use routee_compass_core::model::unit::TimeUnit;

    /// BinRangeConfig with N values produces N-1 bins (sliding window).
    #[test]
    fn bin_range_config_produces_correct_bin_count() {
        let config = BinRangeConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![0, 10, 20, 30],
            unit: TimeUnit::Minutes,
            prepend_zero: true,
        };
        let bins = config.build_bins().unwrap();
        assert_eq!(bins.len(), 3);
    }

    /// Each bin key matches the upper bound of that bin.
    #[test]
    fn bin_keys_match_upper_bounds() {
        let config = BinRangeConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![0, 10, 20, 30],
            unit: TimeUnit::Minutes,
            prepend_zero: true,
        };
        let bins = config.build_bins().unwrap();
        let keys: Vec<String> = bins.iter().map(|b| b.bin_key()).collect();
        assert_eq!(keys, vec!["10", "20", "30"]);
    }

    #[test]
    fn build_bins_rejects_single_value() {
        let config = BinRangeConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![10],
            unit: TimeUnit::Minutes,
            prepend_zero: true,
        };
        assert!(config.build_bins().is_err());
    }

    #[test]
    fn build_bins_rejects_empty_values() {
        let config = BinRangeConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![],
            unit: TimeUnit::Minutes,
            prepend_zero: true,
        };
        assert!(config.build_bins().is_err());
    }

    #[test]
    fn descending_values_are_normalized() {
        let config = BinRangeConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![30, 10, 20, 0],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins().unwrap();
        let keys: Vec<String> = bins.iter().map(|b| b.bin_key()).collect();
        assert_eq!(keys, vec!["10", "20", "30"]);
    }

    #[test]
    fn duplicate_values_are_deduped() {
        let config = BinRangeConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![0, 10, 10, 20],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins().unwrap();
        assert_eq!(bins.len(), 2); // [0,10), [10,20)
    }
}
