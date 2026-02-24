use itertools::Itertools;
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::model::{
    state::{CustomVariableType, StateModel, StateModelError, StateVariable},
    unit::{DistanceUnit, EnergyUnit, TimeUnit},
};
use serde::{Deserialize, Serialize};

use super::bin;

/// configure a set of bins for aggregate isochrone/opportunity models
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinsConfig {
    /// the type of bin to create
    pub bin_types: Vec<BinType>,
}

/// type, unit and feature name of the state variable used for binning
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum BinType {
    Distance {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<f64>,
        /// unit of values
        unit: DistanceUnit,
    },
    Time {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<f64>,
        /// unit of values
        unit: TimeUnit,
    },
    Energy {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<f64>,
        /// unit of values
        unit: EnergyUnit,
    },
    CustomRange {
        /// state model feature name to test with
        feature: String,
        /// values to use when constructing the bins
        values: Vec<f64>,
        /// unit of values
        unit: CustomVariableType,
    },
    Boolean {
        feature: String,
        negate: bool,
    },
}

/// a single bin between for values in the range [min, max).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Bin {
    Distance {
        feature: String,
        min: uom::si::f64::Length,
        max: uom::si::f64::Length,
    },
    Time {
        feature: String,
        min: uom::si::f64::Time,
        max: uom::si::f64::Time,
    },
    Energy {
        feature: String,
        min: uom::si::f64::Energy,
        max: uom::si::f64::Energy,
    },
    CustomRange {
        feature: String,
        unit: CustomVariableType,
        min: f64,
        max: f64,
    },
    Boolean {
        feature: String,
        negate: bool,
    },
}

impl BinType {
    pub fn build_bins(&self) -> Vec<Bin> {
        match self {
            BinType::Distance {
                feature,
                values,
                unit,
            } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| Bin::Distance {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min),
                    max: unit.to_uom(*max),
                })
                .collect_vec(),
            BinType::Time {
                feature,
                values,
                unit,
            } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| Bin::Time {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min),
                    max: unit.to_uom(*max),
                })
                .collect_vec(),
            BinType::Energy {
                feature,
                values,
                unit,
            } => values
                .iter()
                .tuple_windows()
                .map(|(min, max)| Bin::Energy {
                    feature: feature.to_string(),
                    min: unit.to_uom(*min),
                    max: unit.to_uom(*max),
                })
                .collect_vec(),
            BinType::CustomRange {
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
                    .map(|(min, max)| Bin::CustomRange {
                        min: *min,
                        max: *max,
                        feature: feature.to_string(),
                        unit: unit.clone(),
                    })
                    .collect_vec()
            }
            BinType::Boolean { feature, negate } => vec![Bin::Boolean {
                feature: feature.clone(),
                negate: *negate,
            }],
        }
    }
}

impl BinsConfig {
    /// constructs the bins from this configuration.
    pub fn build(&self) -> Vec<Bin> {
        self.bin_types
            .iter()
            .flat_map(|b| b.build_bins())
            .collect_vec()
    }
}

impl Bin {
    /// determine whether a trip state is within some bin
    pub fn within_bin(
        &self,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, StateModelError> {
        match self {
            Bin::Distance { feature, min, max } => {
                let value = state_model.get_distance(state, feature)?;
                let result = within_bin(min, &value, max);
                Ok(result)
            }
            Bin::Time { feature, min, max } => {
                let value = state_model.get_time(state, feature)?;
                let result = within_bin(min, &value, max);
                Ok(result)
            }
            Bin::Energy { feature, min, max } => {
                let value = state_model.get_energy(state, feature)?;
                let result = within_bin(min, &value, max);
                Ok(result)
            }
            Bin::CustomRange {
                feature,
                unit,
                min,
                max,
            } => match unit {
                CustomVariableType::FloatingPoint => {
                    let value = state_model.get_custom_f64(state, feature)?;
                    Ok(within_bin(min, &value, max))
                }
                CustomVariableType::SignedInteger => {
                    let value = state_model.get_custom_i64(state, feature)?;
                    Ok(within_bin(min, &(value as f64), max))
                }
                CustomVariableType::UnsignedInteger => {
                    let value = state_model.get_custom_u64(state, feature)?;
                    Ok(within_bin(min, &(value as f64), max))
                }
                CustomVariableType::Boolean => {
                    let value = state_model.get_custom_bool(state, feature)?;
                    Ok(within_bin(min, &((value as i64) as f64), max))
                }
            },
            Bin::Boolean { feature, negate } => {
                let value = state_model.get_custom_bool(state, feature)?;
                Ok(if *negate { !value } else { value })
            }
        }
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
