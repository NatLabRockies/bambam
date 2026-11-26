use std::collections::HashMap;

use routee_compass_core::model::unit::SpeedUnit;
use serde::{Deserialize, Serialize};

use crate::lts::lts::Lts;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct LtsConfig {
    /// file containing LTS values for each edge.
    pub lts_input_file: String,
    /// speed unit to store lts speeds.
    pub speed_unit: SpeedUnit,
    /// default mapping for LTS values to speed ratios
    pub mapping: Option<LtsMappingConfig>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LtsOperationConfig {
    /// for a given LTS value, the percentage of speed retained for LTS in `(0.0, 100.0]`.
    /// for example, if an edge has a bike speed of 10mph
    /// and this value is 80.0, then the resulting speed will
    /// be 8mph.
    PercentPenalty { percent: f64 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LtsMappingConfig(pub HashMap<Lts, LtsOperationConfig>);
