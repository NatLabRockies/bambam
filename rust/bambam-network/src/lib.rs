//! Shared split-based road network ETL, island detection, spatial infrastructure attachment,
//! and RouteE Compass serialization core.
//!
//! Designed with zero BAMBAM/census coupling so it can be upstreamed directly into
//! `routee-compass`. Both `bambam-omf` (Overture Maps Foundation) and `bambam-osm`
//! (OpenStreetMap) implement [`ImportConnector`], [`ImportInfrastructure`], and
//! [`ImportSegment`] to share this pipeline.

pub mod consts;
pub mod error;
pub mod fs;
pub mod island_detection;
pub mod model;
pub mod segment_split;
pub mod serialize_ops;
pub mod summary;
pub mod vertex_serializable;

pub use consts::*;
pub use error::NetworkImportError;
pub use island_detection::{ComponentsAlgorithmType, IslandDetectionAlgorithm};
pub use model::{
    ConnectorInSegment, ImportConnector, ImportInfrastructure, ImportSegment, SegmentHeading,
};
pub use segment_split::SegmentSplit;
pub use serialize_ops::VectorizedEdgeList;
pub use summary::{ClassStats, EdgeListStats};
pub use vertex_serializable::{
    attach_infrastructure_to_vertices, EdgeMappingRecord, NetworkVertexComplete, OmfVertexComplete,
    VertexMappingRecord, VertexSerializable, DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS,
    INFRASTRUCTURE_ID_DELIMITER,
};
