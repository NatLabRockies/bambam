use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use crate::app::network::lts::compute_lts::compute_lts;
use crate::app::network::lts::lts::Lts;
use crate::app::network::lts::lts::LtsError;
use crate::app::network::wci::compute_wci::{compute_wci, WciComponents};
use crate::app::network::wci::wci::WciError;
use crate::model::osm::graph::OsmNodeDataSerializable;
use rstar::RTree;
use std::{error::Error, io::Write, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModalMetricError {
    #[error(transparent)]
    Wci(#[from] WciError),

    #[error(transparent)]
    Lts(#[from] LtsError),

    #[error("Invalid modal metric name: {0}")]
    InvalidModalMetric(String),

    #[error("{0}")]
    Other(String),
}

/// Supported modal metric calculation options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalMetric {
    WalkingComfortIndex,
    LevelOfTrafficStress,
}

/// Actual computed values for a modal metric.
pub enum ModalMetricValue {
    Wci(WciComponents),
    Lts(Lts),
}

impl FromStr for ModalMetric {
    type Err = ModalMetricError;

    fn from_str(s: &str) -> Result<ModalMetric, ModalMetricError> {
        match s.to_ascii_uppercase().as_str() {
            "WCI" => Ok(ModalMetric::WalkingComfortIndex),
            "LTS" => Ok(ModalMetric::LevelOfTrafficStress),
            _ => Err(ModalMetricError::InvalidModalMetric(format!(
                "Unsupported metric name: {}",
                s,
            ))),
        }
    }
}

impl ModalMetric {
    /// Computes the specified modal metric for the given way entry and source node.
    pub fn compute_metric(
        &self,
        rtree: &RTree<WayRTreeEntry>,
        way_entry: &WayRTreeEntry,
        src_node: &OsmNodeDataSerializable,
    ) -> Result<ModalMetricValue, ModalMetricError> {
        match self {
            ModalMetric::WalkingComfortIndex => {
                let wci = compute_wci(rtree, way_entry, src_node)?;
                Ok(ModalMetricValue::Wci(wci))
            }
            ModalMetric::LevelOfTrafficStress => {
                let lts = compute_lts(rtree, way_entry)?;
                Ok(ModalMetricValue::Lts(lts))
            }
        }
    }

    pub fn write_csv_header(&self, writer: &mut impl Write) -> Result<(), Box<dyn Error>> {
        match self {
            ModalMetric::WalkingComfortIndex => {
                writeln!(writer, "wci_total,wci_walk,wci_speed,wci_cycle,wci_signal")?;
            }
            ModalMetric::LevelOfTrafficStress => {
                writeln!(writer, "lts")?;
            }
        }
        Ok(())
    }
}

impl ModalMetricValue {
    pub fn write_csv_row(&self, writer: &mut impl Write) -> Result<(), Box<dyn Error>> {
        match self {
            ModalMetricValue::Wci(wci) => {
                writeln!(
                    writer,
                    "{},{},{},{},{}",
                    wci.total,
                    wci.walkability
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                    wci.traffic_speed_comfort
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                    wci.cycleway_comfort
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                    wci.traffic_signal_comfort
                        .as_ref()
                        .map_or(String::new(), |v| v.to_string()),
                )?;
            }
            ModalMetricValue::Lts(lts) => {
                writeln!(writer, "{}", lts)?;
            }
        }
        Ok(())
    }
}
