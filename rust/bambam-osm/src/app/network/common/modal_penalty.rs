use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use crate::app::network::lts::lts_score::LtsError;
use crate::app::network::lts::lts_score::LtsScore;
use crate::app::network::wci::compute_wci::{compute_wci, WciComponentScores};
use crate::app::network::wci::wci_score::WciError;
use crate::model::osm::graph::OsmNodeDataSerializable;
use rstar::RTree;
use std::{error::Error, io::Write, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModalPenaltyError {
    #[error(transparent)]
    Wci(#[from] WciError),

    #[error(transparent)]
    Lts(#[from] LtsError),

    #[error("Invalid modal penalty name: {0}")]
    InvalidModalPenalty(String),

    #[error("{0}")]
    Other(String),
}

/// Supported modal penalty calculation options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalPenalty {
    WalkingComfortIndex,
    LevelOfTrafficStress,
}

impl FromStr for ModalPenalty {
    type Err = ModalPenaltyError;

    fn from_str(s: &str) -> Result<ModalPenalty, ModalPenaltyError> {
        match s.to_ascii_uppercase().as_str() {
            "WCI" => Ok(ModalPenalty::WalkingComfortIndex),
            "LTS" => Ok(ModalPenalty::LevelOfTrafficStress),
            _ => Err(ModalPenaltyError::InvalidModalPenalty(format!(
                "Unsupported penalty name: {}",
                s,
            ))),
        }
    }
}

impl ModalPenalty {
    /// Compute the modal penalty score for a single way entry.
    pub fn compute_score(
        &self,
        rtree: &RTree<WayRTreeEntry>,
        way_entry: &WayRTreeEntry,
        src_node: &OsmNodeDataSerializable,
    ) -> Result<ModalPenaltyResult, ModalPenaltyError> {
        match self {
            ModalPenalty::WalkingComfortIndex => {
                let wci = compute_wci(rtree, way_entry, src_node)?;
                Ok(ModalPenaltyResult::Wci(wci))
            }
            ModalPenalty::LevelOfTrafficStress => {
                let lts =
                    crate::app::network::lts::compute_lts::compute_lts(rtree, way_entry, src_node)?;
                Ok(ModalPenaltyResult::Lts(lts))
            }
        }
    }

    /// Write the CSV header appropriate for this modal penalty variant.
    pub fn write_csv_header(&self, writer: &mut impl Write) -> Result<(), Box<dyn Error>> {
        match self {
            ModalPenalty::WalkingComfortIndex => {
                writeln!(writer, "wci_total,wci_walk,wci_speed,wci_cycle,wci_signal")?;
            }
            ModalPenalty::LevelOfTrafficStress => {
                writeln!(writer, "lts")?;
            }
        }
        Ok(())
    }
}

/// The computed result for a single way entry.
pub enum ModalPenaltyResult {
    Wci(WciComponentScores),
    Lts(LtsScore), // To be added when LTS is implemented
}

impl ModalPenaltyResult {
    /// Write the result row to CSV writer.
    pub fn write_csv_row(&self, writer: &mut impl Write) -> Result<(), Box<dyn Error>> {
        match self {
            ModalPenaltyResult::Wci(wci) => {
                writeln!(
                    writer,
                    "{},{},{},{},{}",
                    wci.total_score,
                    wci.walkability_score
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                    wci.traffic_speed_score
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                    wci.cycleway_score
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                    wci.traffic_signal_score
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                )?;
            }
            ModalPenaltyResult::Lts(lts) => {
                writeln!(writer, "{}", lts)?;
            }
        }
        Ok(())
    }
}
