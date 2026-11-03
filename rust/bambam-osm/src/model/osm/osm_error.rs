use thiserror::Error;

use super::graph::{OsmNodeId, OsmWayId};

#[derive(Error, Debug)]
pub enum OsmError {
    #[error("invalid OSM import configuration: {0}")]
    ConfigurationError(String),
    #[error("failure reading .pbf file: {source}")]
    PbfLibError { source: osmpbf::Error },
    #[error("failure writing to file {0}: {1}")]
    CsvWriteError(String, csv::Error),
    #[error("attempting to get node '{0}' not in graph")]
    GraphMissingNodeId(OsmNodeId),
    #[error("attempting to get way '{0}' not in graph")]
    GraphMissingWayId(OsmWayId),
    #[error("failure while serializing graph: {0}")]
    GraphSerializationError(String),
    #[error("structure of OSM data is invalid: {0}")]
    InvalidOsmData(String),
    #[error("pbf does not contain any OSM 'node' elements")]
    NoNodesFound,
    #[error("pbf does not contain any OSM 'way' elements")]
    NoWaysFound,
    #[error("unable to deserialize WKT into geometry: {0}")]
    InvalidWKT(String),
    #[error("Geometry of WKT is not a valid extent: {0}")]
    InvalidExtentWKT(String),
    #[error("{0}")]
    InternalError(String),
}

impl From<bambam_network::NetworkImportError> for OsmError {
    fn from(value: bambam_network::NetworkImportError) -> Self {
        use bambam_network::NetworkImportError as N;
        match value {
            N::InvalidUserInput(s) | N::MissingAttribute(s) | N::InvalidBetweenVector(s) => {
                Self::ConfigurationError(s)
            }
            N::InvalidGeometry(s) => Self::InvalidWKT(s),
            N::SerializationError(s) | N::CsvWriteError(s) => Self::GraphSerializationError(s),
            N::InvalidSegmentConnectors(s) => Self::InvalidOsmData(s),
            N::InvalidLinearReference(v) => {
                Self::InvalidOsmData(format!("invalid linear reference {v}"))
            }
            N::ReadError { path, message } => {
                Self::ConfigurationError(format!("error reading '{}': {message}", path.display()))
            }
            N::WriteError { path, message } => Self::GraphSerializationError(format!(
                "error writing '{}': {message}",
                path.display()
            )),
            N::InternalError(s) => Self::InternalError(s),
        }
    }
}
