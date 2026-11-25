use crate::wci::wci::Wci;

use super::WciConfig;

use kdam::BarBuilder;
use routee_compass_core::{
    model::{constraint::ConstraintModelError, network::Edge},
    util::fs::read_utils,
};

pub struct WciEngine {
    values: Box<[Wci]>,
}

impl TryFrom<WciConfig> for WciEngine {
    type Error = ConstraintModelError;

    fn try_from(config: WciConfig) -> Result<Self, Self::Error> {
        let bar_builder = BarBuilder::default().desc("wci values");
        let op = |_idx: usize, row: String| {
            let wci_val = row.parse().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("bad WCI value '{row}': {e}"),
                )
            })?;
            Wci::new(wci_val).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("bad WCI value '{row}': {e}"),
                )
            })
        };
        let values = read_utils::read_raw_file(&config.wci_input_file, op, Some(bar_builder), None)
            .map_err(|e| {
                ConstraintModelError::BuildError(format!(
                    "failed reading {}: {e}",
                    config.wci_input_file
                ))
            })?;
        Ok(Self { values })
    }
}

impl WciEngine {
    /// test if the given edge has a WCI which is equal to or greater than our threshold.
    pub fn accept_edge_wci(
        &self,
        edge: &Edge,
        threshold_inclusive: Wci,
    ) -> Result<bool, ConstraintModelError> {
        let value: Wci = self
            .values
            .get(edge.edge_id.as_usize())
            .cloned()
            .ok_or_else(|| {
                let msg = format!("edge {} missing from WCI dataset", edge.edge_id);
                ConstraintModelError::ConstraintModelError(msg)
            })?;
        Ok(value >= threshold_inclusive)
    }
}
