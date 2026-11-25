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
    /// for a given LTS value, the speed ratio for LTS.
    /// for example, if an edge has a bike speed of 10mph
    /// and this value is 0.8, then the resulting speed will
    /// be 8mph.
    PercentPenalty { percent: f64 },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LtsMappingConfig(pub HashMap<Lts, LtsOperationConfig>);

impl Clone for LtsMappingConfig {
    fn clone(&self) -> Self {
        let mut inner = HashMap::with_capacity(self.0.len());
        for (k, v) in self.0.iter() {
            inner.insert(*k, v.clone());
        }
        Self(inner)
    }
}
