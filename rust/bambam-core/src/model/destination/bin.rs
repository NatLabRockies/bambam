use itertools::Itertools;
use routee_compass_core::model::{
    state::{CustomVariableType, StateModel, StateVariable},
    unit::{DistanceUnit, EnergyUnit, TimeUnit},
};
use serde::{Deserialize, Serialize};
use uom::ConstZero;

use crate::model::destination::DestinationError;

/// configures a bambam run to produce aggregated opportunity insights
/// in bins at some interval.
///
/// a set of bins is built based on a feature in the state vector, which is expected
/// to match one of Distance, Time, Energy, or Custom di
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BinningConfig {
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
pub enum BinInterval {
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

impl BinningConfig {
    /// Accessor for the values slice, regardless of variant, as an always-ascending
    /// list of numbers with no duplicates.
    fn values_ascending(&self) -> Vec<u64> {
        let mut values = match self {
            BinningConfig::Distance { values, .. }
            | BinningConfig::Time { values, .. }
            | BinningConfig::Energy { values, .. }
            | BinningConfig::CustomRange { values, .. } => values.clone(),
        };
        values.sort();
        values.dedup();
        values
    }

    /// true if the configured binning values should have a zero value prepended.
    fn prepend_zero(&self) -> bool {
        match self {
            BinningConfig::Distance { prepend_zero, .. }
            | BinningConfig::Time { prepend_zero, .. }
            | BinningConfig::Energy { prepend_zero, .. }
            | BinningConfig::CustomRange { prepend_zero, .. } => *prepend_zero,
        }
    }

    /// create the collection of bins from this configuration. each of these bins will capture
    /// a subset of the destinations.
    ///
    /// # Arguments
    ///
    /// * `marginal` - if true, each bin interval will be bounded by two consecutive values in the
    /// binning config. if false, each bin will start with zero.
    pub fn build_bins(&self, marginal: bool) -> Result<Vec<BinInterval>, DestinationError> {
        let mut values = self.values_ascending();
        if self.prepend_zero() || !marginal {
            values.push(0);
        }
        values.sort();
        values.dedup();
        if values.len() < 2 {
            return Err(DestinationError::InvalidBinConfig {
                reason: format!(
                    "bin range config requires at least 2 values to form a bin, got {}",
                    values.len()
                ),
            });
        }

        let bins = match self {
            BinningConfig::Distance { feature, unit, .. } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinInterval::Distance {
                    feature: feature.to_string(),
                    min: if marginal {
                        unit.to_uom(*min as f64)
                    } else {
                        uom::si::f64::Length::ZERO
                    },
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinningConfig::Time { feature, unit, .. } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinInterval::Time {
                    feature: feature.to_string(),
                    min: if marginal {
                        unit.to_uom(*min as f64)
                    } else {
                        uom::si::f64::Time::ZERO
                    },
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinningConfig::Energy { feature, unit, .. } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| BinInterval::Energy {
                    feature: feature.to_string(),
                    min: if marginal {
                        unit.to_uom(*min as f64)
                    } else {
                        uom::si::f64::Energy::ZERO
                    },
                    max: unit.to_uom(*max as f64),
                    unit: *unit,
                })
                .collect_vec(),
            BinningConfig::CustomRange { feature, unit, .. } => {
                if matches!(unit, CustomVariableType::Boolean) {
                    let msg = format!("ranged bin for feature '{feature}' is a boolean, but boolean cardinality is 2, which is not large enough to support bins. this may produce undefined behavior.");
                    log::warn!("{}", msg);
                }
                values
                    .iter()
                    .tuple_windows()
                    .map(|(min, max)| BinInterval::CustomRange {
                        min: if marginal { *min as f64 } else { 0.0 },
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

impl BinInterval {
    /// Returns a stable string key for this bin derived from its upper bound in
    /// the configured unit, rounded to the nearest integer.  For a `Time` bin
    /// with max = 10 minutes this returns `"10"`, matching the previous
    /// `TimeBin::key()` convention.
    pub fn bin_key(&self) -> String {
        match self {
            BinInterval::Time { max, unit, .. } => {
                format!("{}", unit.from_uom(*max).round() as u64)
            }
            BinInterval::Distance { max, unit, .. } => {
                format!("{}", unit.from_uom(*max).round() as u64)
            }
            BinInterval::Energy { max, unit, .. } => {
                format!("{}", unit.from_uom(*max).round() as u64)
            }
            BinInterval::CustomRange { max, .. } => {
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
            BinInterval::Distance {
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
            BinInterval::Time {
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
            BinInterval::Energy {
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
            BinInterval::CustomRange {
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

impl std::fmt::Display for BinInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinInterval::Distance {
                feature,
                min,
                max,
                unit,
            } => {
                let (min_v, max_v) = (unit.from_uom(*min), unit.from_uom(*max));
                format!("{feature} in [{min_v}, {max_v}) {unit}")
            }
            BinInterval::Time {
                feature,
                min,
                max,
                unit,
            } => {
                let (min_v, max_v) = (unit.from_uom(*min), unit.from_uom(*max));
                format!("{feature} in [{min_v}, {max_v}) {unit}")
            }
            BinInterval::Energy {
                feature,
                min,
                max,
                unit,
            } => {
                let (min_v, max_v) = (unit.from_uom(*min), unit.from_uom(*max));
                format!("{feature} in [{min_v}, {max_v}) {unit}")
            }
            BinInterval::CustomRange {
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
    use super::{BinInterval, BinningConfig};
    use routee_compass_core::model::unit::TimeUnit;
    use uom::si::{f64::Time, time::minute};

    /// BinRangeConfig with N values produces N-1 bins (sliding window).
    #[test]
    fn bin_range_config_produces_correct_bin_count() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![0, 10, 20, 30],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins(true).unwrap();
        assert_eq!(bins.len(), 3);
    }

    /// marginal = false
    #[test]
    fn build_bins_not_marginal() {
        let feature = "travel_time".to_string();
        let config = BinningConfig::Time {
            feature: feature.clone(),
            values: vec![0, 10, 20, 30],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins(false).unwrap();
        for (bin, expected_max) in bins.into_iter().zip([10.0, 20.0, 30.0]) {
            match bin {
                BinInterval::Time { min, max, .. } => {
                    assert_eq!(min, Time::new::<minute>(0.0));
                    assert_eq!(max, Time::new::<minute>(expected_max));
                }
                _ => panic!("unexpected bin type"),
            }
        }
    }

    #[test]
    fn build_bins_not_marginal_prepends_zero_when_enabled() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![10, 20, 30],
            unit: TimeUnit::Minutes,
            prepend_zero: true,
        };
        let bins = config.build_bins(false).unwrap();
        for (bin, expected_max) in bins.into_iter().zip([10.0, 20.0, 30.0]) {
            match bin {
                BinInterval::Time { min, max, .. } => {
                    assert_eq!(min, Time::new::<minute>(0.0));
                    assert_eq!(max, Time::new::<minute>(expected_max));
                }
                _ => panic!("unexpected bin type"),
            }
        }
    }

    #[test]
    fn build_bins_not_marginal_without_zero_and_no_prepend_uses_first_value() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![10, 20, 30],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins(false).unwrap();
        for (bin, expected_max) in bins.into_iter().zip([10.0, 20.0, 30.0]) {
            match bin {
                BinInterval::Time { min, max, .. } => {
                    assert_eq!(min, Time::new::<minute>(0.0));
                    assert_eq!(max, Time::new::<minute>(expected_max));
                }
                _ => panic!("unexpected bin type"),
            }
        }
    }

    /// Each bin key matches the upper bound of that bin.
    #[test]
    fn bin_keys_match_upper_bounds() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![0, 10, 20, 30],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins(true).unwrap();
        let keys: Vec<String> = bins.iter().map(|b| b.bin_key()).collect();
        assert_eq!(keys, vec!["10", "20", "30"]);
    }

    #[test]
    fn build_bins_rejects_single_value() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![10],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        assert!(config.build_bins(true).is_err());
    }

    #[test]
    fn build_bins_rejects_empty_values() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![],
            unit: TimeUnit::Minutes,
            prepend_zero: true,
        };
        assert!(config.build_bins(true).is_err());
    }

    #[test]
    fn descending_values_are_normalized() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![30, 10, 20, 0],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins(true).unwrap();
        let keys: Vec<String> = bins.iter().map(|b| b.bin_key()).collect();
        assert_eq!(keys, vec!["10", "20", "30"]);
    }

    #[test]
    fn duplicate_values_are_deduped() {
        let config = BinningConfig::Time {
            feature: "travel_time".to_string(),
            values: vec![0, 10, 10, 20],
            unit: TimeUnit::Minutes,
            prepend_zero: false,
        };
        let bins = config.build_bins(true).unwrap();
        assert_eq!(bins.len(), 2); // [0,10), [10,20)
    }
}
