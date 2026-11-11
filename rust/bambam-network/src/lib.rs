//! Shared split-based road network ETL, island detection, spatial infrastructure attachment,
//! and RouteE Compass serialization core.
//!
//! Designed with zero BAMBAM/census coupling so it can be upstreamed directly into
//! `routee-compass`. Both `bambam-omf` (Overture Maps Foundation) and `bambam-osm`
//! (OpenStreetMap) implement [`ImportConnector`], [`ImportInfrastructure`], and
//! [`ImportSegment`] to share this pipeline.

pub mod cli;
pub mod consts;
pub mod edge_complete_record;
pub mod edge_mapping_record;
pub mod error;
pub mod fs;
pub mod island_detection;
pub mod model;
pub mod segment_split;
pub mod serialize_ops;
pub mod summary;
pub mod taxonomy;
pub mod vertex_compass_record;
pub mod vertex_complete_record;
pub mod vertex_mapping_record;

pub use cli::{
    parse_wkt_str_to_geometry_and_bbox, read_extent_file_to_geometry_and_bbox,
    CommonNetworkImportArgs, NetworkImportFilterConfig,
};
pub use consts::*;
pub use edge_complete_record::{
    deserialize_linestring_wkt, serialize_linestring_wkt, EdgeCompleteRecord,
};
pub use edge_mapping_record::EdgeMappingRecord;
pub use error::NetworkImportError;
pub use island_detection::{ComponentsAlgorithmType, IslandDetectionAlgorithm};
pub use model::{
    ConnectorInSegment, ImportConnector, ImportInfrastructure, ImportSegment, SegmentHeading,
};
pub use segment_split::SegmentSplit;
pub use serialize_ops::{build_vectorized_network_graph, BuiltNetworkGraph, VectorizedEdgeList};
pub use summary::{
    write_summary, ClassStats, EdgeListStats, NetworkGraphSource, NetworkGraphStats,
    NetworkGraphSummary,
};
pub use taxonomy::{
    evaluate_segment_headings, CliBoundingBox, DuringExpression, FilterableSegment, MatchBehavior,
    NetworkEdgeListConfiguration, SegmentAccessRestriction, SegmentAccessRestrictionWhen,
    SegmentAccessRestrictionWhenVehicle, SegmentAccessType, SegmentClass, SegmentFullType,
    SegmentImperialWeightUnit, SegmentLengthUnit, SegmentMetricWeightUnit, SegmentMode,
    SegmentRecognized, SegmentSubclass, SegmentSubtype, SegmentUnit, SegmentUsing,
    SegmentVehicleComparator, SegmentVehicleDimension, SegmentWeightUnit, TravelModeFilter,
};
pub use vertex_compass_record::VertexCompassRecord;
pub use vertex_complete_record::{
    attach_infrastructure_to_vertices, VertexCompleteRecord,
    DEFAULT_INFRASTRUCTURE_MATCH_DISTANCE_METERS, INFRASTRUCTURE_ID_DELIMITER,
};
pub use vertex_mapping_record::VertexMappingRecord;
