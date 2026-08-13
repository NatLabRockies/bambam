use crate::common::edge_rtree_entry::EdgeRTreeEntry;
use crate::lts::compute_lts::compute_lts;
use crate::lts::lts::Lts;
use crate::lts::lts::LtsError;
use crate::network_traits::{
    edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge,
    vertex_for_modal_metric::VertexForModalMetric,
};
use crate::wci::compute_wci::{compute_wci, WciComponents};
use crate::wci::wci::WciError;
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
            _ => Err(ModalMetricError::InvalidModalMetric(s.to_string())),
        }
    }
}

impl ModalMetric {
    /// Computes the specified modal metric for the given edge entry and source vertex.
    pub fn compute_metric<E, V>(
        &self,
        rtree: &RTree<EdgeRTreeEntry<E>>,
        edge_entry: &EdgeRTreeEntry<E>,
        src_vertex: Option<&V>,
    ) -> Result<ModalMetricValue, ModalMetricError>
    where
        E: SpatialEdge + EdgeForModalMetric,
        V: VertexForModalMetric,
    {
        match self {
            ModalMetric::WalkingComfortIndex => {
                let wci = compute_wci(rtree, edge_entry, src_vertex)?;
                Ok(ModalMetricValue::Wci(wci))
            }
            ModalMetric::LevelOfTrafficStress => {
                let lts = compute_lts(rtree, edge_entry)?;
                Ok(ModalMetricValue::Lts(lts))
            }
        }
    }
    /// Writes the CSV header for the specified modal metric.
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
