use std::collections::HashMap;

use crate::{lts::lts::Lts, traversal::lts::config::LtsOperationConfig};

use super::LtsConfig;

use itertools::Itertools;
use kdam::BarBuilder;
use routee_compass_core::{
    model::{network::EdgeId, traversal::TraversalModelError},
    util::fs::read_utils,
};
use uom::si::f64::{Ratio, Velocity};

pub struct LtsEngine {
    values: Box<[Lts]>,
    pub config: LtsConfig,
}

pub type LtsMappingOps = HashMap<Lts, LtsOp>;

pub enum LtsOp {
    RatioPenalty { ratio: Ratio },
}

impl TryFrom<&LtsOperationConfig> for LtsOp {
    type Error = TraversalModelError;

    fn try_from(value: &LtsOperationConfig) -> Result<Self, Self::Error> {
        match value {
            LtsOperationConfig::PercentPenalty { percent } => {
                if !percent.is_finite() || *percent <= 0.0 || *percent > 100.0 {
                    return Err(TraversalModelError::BuildError(format!(
                        "LTS percent penalty must be in (0.0, 100.0], found {percent}"
                    )));
                }
                let ratio = Ratio::new::<uom::si::ratio::percent>(*percent);
                Ok(Self::RatioPenalty { ratio })
            }
        }
    }
}

impl LtsOp {
    pub fn apply(&self, speed: Velocity) -> Velocity {
        match self {
            LtsOp::RatioPenalty { ratio } => speed * ratio.get::<uom::si::ratio::ratio>(),
        }
    }
}

impl LtsEngine {
    pub fn get_lts_speed(
        &self,
        edge_id: EdgeId,
        edge_speed: Velocity,
        mapping: &LtsMappingOps,
    ) -> Result<Velocity, TraversalModelError> {
        let value: Lts = self
            .values
            .get(edge_id.as_usize())
            .cloned()
            .ok_or_else(|| {
                let msg = format!("edge {} missing from LTS dataset", edge_id);
                TraversalModelError::TraversalModelFailure(msg)
            })?;
        let op = mapping.get(&value).ok_or_else(|| {
            let found = mapping.keys().map(|k| k.to_string()).join(", ");
            let msg = format!(
                "LTS value '{value}' not present in mapping. found these LTS values: [{found}]."
            );
            TraversalModelError::TraversalModelFailure(msg)
        })?;
        let result = op.apply(edge_speed);
        Ok(result)
    }
}

impl TryFrom<LtsConfig> for LtsEngine {
    type Error = TraversalModelError;

    fn try_from(config: LtsConfig) -> Result<Self, Self::Error> {
        let bar_builder = BarBuilder::default().desc("lts values");
        let op = |_idx: usize, row: String| {
            let lts_val = row.parse().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("bad LTS value '{row}': {e}"),
                )
            })?;
            Lts::new(lts_val).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("bad LTS value '{row}': {e}"),
                )
            })
        };
        let values = read_utils::read_raw_file(&config.lts_input_file, op, Some(bar_builder), None)
            .map_err(|e| {
                TraversalModelError::BuildError(format!(
                    "failed reading {}: {e}",
                    config.lts_input_file
                ))
            })?;

        Ok(Self { values, config })
    }
}
