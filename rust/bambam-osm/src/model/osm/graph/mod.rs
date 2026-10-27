mod compass_writer;
pub mod osm_element_filter;
mod osm_graph_vectorized;
pub mod osm_node_data;
mod osm_node_data_serializable;
mod osm_node_id;
pub mod osm_way_data;
mod osm_way_data_serializable;
mod osm_way_id;
pub mod osm_way_ops;

pub use bambam_network::{
    EdgeMappingRecord, VertexCompassRecord, VertexCompleteRecord, VertexMappingRecord,
};
pub use compass_writer::CompassWriter;
pub use osm_graph_vectorized::OsmGraphVectorized;
pub use osm_node_data::{OsmConnectorRecord, OsmNodeData};
pub use osm_node_data_serializable::OsmNodeDataSerializable;
pub use osm_node_id::OsmNodeId;
pub use osm_way_data::OsmWayData;
pub use osm_way_data_serializable::OsmWayDataSerializable;
pub use osm_way_id::OsmWayId;
use routee_compass_core::model::network::Vertex;
use std::collections::HashMap;

pub type Osmid = i64;
pub type OsmNodes = HashMap<OsmNodeId, OsmNodeData>;
pub type OsmWays = HashMap<OsmWayId, OsmWayData>;
