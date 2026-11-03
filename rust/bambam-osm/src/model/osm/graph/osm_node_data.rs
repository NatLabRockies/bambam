use geo::{Geometry, Intersects, Point};
use itertools::Itertools;
use routee_compass_core::model::network::Vertex;
use serde::{Deserialize, Serialize};

use super::OsmNodeId;

/// represents an OSM node. this may be the original raw record or an aggregated
/// record. raw records have no "consolidated_ids".
///
/// if this is an aggregated record, then:
/// - consolidated_ids contains the [`OsmNodeId`]s of any subsumed ids
/// - x and y positions are the centroid of the subsumed nodes
/// - "highway", "ele", "junction", "railway", and "_ref" attributes
///   are whitespace-delimited strings of the unique values from all
///   subsumed nodes.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct OsmNodeData {
    pub osmid: OsmNodeId,
    pub x: f32,
    pub y: f32,
    pub highway: Option<String>,
    pub ele: Option<String>,
    pub junction: Option<String>,
    pub railway: Option<String>,
    pub _ref: Option<String>,
    /// when nodes are consolidated, the list of composite node ids are stored here
    pub consolidated_ids: Vec<OsmNodeId>,
}

impl OsmNodeData {
    /// a delimter for aggregated fields which does not collide with any known/expected OSM values
    pub const VALUE_DELIMITER: &'static str = "#";

    /// decodes the `ele` value(s) present at this node
    pub fn get_elevation(&self) -> Option<f64> {
        self.ele.clone().map(|ele| {
            let values = ele
                .split(Self::VALUE_DELIMITER)
                .flat_map(parse_ele)
                .collect_vec();
            let mean_elevation = values.iter().sum::<f64>() / values.len() as f64;
            mean_elevation
        })
    }

    pub fn get_point(&self) -> Point<f32> {
        Point::new(self.x, self.y)
    }

    pub fn has_traffic_signals(&self) -> bool {
        self.highway
            .as_ref()
            .is_some_and(|h| h.contains("traffic_signals"))
    }

    pub fn has_stop(&self) -> bool {
        self.highway.as_ref().is_some_and(|h| h.contains("stop"))
    }

    pub fn has_give_way(&self) -> bool {
        self.highway
            .as_ref()
            .is_some_and(|h| h.contains("give_way"))
    }

    pub fn has_crossing(&self) -> bool {
        self.highway
            .as_ref()
            .is_some_and(|h| h.contains("crossing"))
    }

    pub fn has_control_or_intersection_tag(&self) -> bool {
        self.has_traffic_signals()
            || self.has_stop()
            || self.has_give_way()
            || self.has_crossing()
            || self.junction.is_some()
            || self.railway.is_some()
    }
}

impl Intersects<Geometry<f32>> for OsmNodeData {
    fn intersects(&self, rhs: &Geometry<f32>) -> bool {
        rhs.intersects(&geo::Point::new(self.x, self.y))
    }
}

/// Wrapper around `OsmNodeData` with a cached string ID to implement
/// `bambam_network::ImportConnector` and `bambam_network::ImportInfrastructure`.
#[derive(Debug, Clone)]
pub struct OsmConnectorRecord {
    pub id_str: String,
    pub node: OsmNodeData,
}

impl OsmConnectorRecord {
    pub fn new(node: OsmNodeData) -> Self {
        Self {
            id_str: node.osmid.to_string(),
            node,
        }
    }
}

impl bambam_network::ImportConnector for OsmConnectorRecord {
    fn connector_id(&self) -> &str {
        &self.id_str
    }

    fn try_to_vertex(&self, idx: usize) -> Result<Vertex, bambam_network::NetworkImportError> {
        Ok(Vertex::new(idx, self.node.x, self.node.y))
    }

    fn has_traffic_signals(&self) -> bool {
        self.node.has_traffic_signals()
    }

    fn has_stop(&self) -> bool {
        self.node.has_stop()
    }

    fn has_give_way(&self) -> bool {
        self.node.has_give_way()
    }

    fn has_crossing(&self) -> bool {
        self.node.has_crossing()
    }

    fn consolidated_infrastructure_ids(&self) -> Option<Vec<String>> {
        if self.node.consolidated_ids.is_empty() {
            None
        } else {
            Some(
                self.node
                    .consolidated_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect(),
            )
        }
    }
}

impl bambam_network::ImportInfrastructure for OsmConnectorRecord {
    fn infrastructure_id(&self) -> &str {
        &self.id_str
    }

    fn reference_point(&self) -> Option<Point<f32>> {
        Some(self.node.get_point())
    }

    fn is_traffic_signals(&self) -> bool {
        self.node.has_traffic_signals()
    }

    fn is_stop(&self) -> bool {
        self.node.has_stop()
    }

    fn is_give_way(&self) -> bool {
        self.node.has_give_way()
    }

    fn is_crossing(&self) -> bool {
        self.node.has_crossing()
    }
}

impl From<&osmpbf::elements::Node<'_>> for OsmNodeData {
    fn from(node: &osmpbf::elements::Node) -> Self {
        let mut out = OsmNodeData {
            osmid: OsmNodeId(node.id()),
            x: node.lon() as f32,
            y: node.lat() as f32,
            ..Default::default()
        };
        for (k, v) in node.tags() {
            match k {
                "highway" => out.highway = Some(String::from(v)),
                "junction" => out.junction = Some(String::from(v)),
                "railway" => out.railway = Some(String::from(v)),
                // https://wiki.openstreetmap.org/wiki/Key:ele
                "ele" => out.ele = Some(String::from(v)),
                "ele:ft" => out.ele = v.parse::<f64>().ok().map(|f| format!("{}", f * 1.60934)),
                "ref" => out._ref = Some(String::from(v)),
                _ => {}
            }
        }
        out
    }
}

impl From<&osmpbf::dense::DenseNode<'_>> for OsmNodeData {
    fn from(node: &osmpbf::dense::DenseNode<'_>) -> Self {
        let mut out = OsmNodeData {
            osmid: OsmNodeId(node.id()),
            x: node.lon() as f32,
            y: node.lat() as f32,
            ..Default::default()
        };
        for (k, v) in node.tags() {
            match k {
                "highway" => out.highway = Some(String::from(v)),
                "junction" => out.junction = Some(String::from(v)),
                "railway" => out.railway = Some(String::from(v)),
                // https://wiki.openstreetmap.org/wiki/Key:ele
                "ele" => out.ele = Some(String::from(v)),
                "ele:ft" => out.ele = v.parse::<f64>().ok().map(|f| format!("{}", f * 1.60934)),
                "ref" => out._ref = Some(String::from(v)),
                _ => {}
            }
        }
        out
    }
}

fn parse_ele(ele: &str) -> Option<f64> {
    ele.parse::<i64>()
        .map(|i| i as f64)
        .or_else(|_| ele.parse::<f64>())
        .ok()
}
