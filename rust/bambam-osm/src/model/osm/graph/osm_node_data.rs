use crate::model::osm::OsmError;
use geo::{Centroid, Coord, Geometry, Intersects, MultiPoint, Point};
use itertools::Itertools;
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

    pub fn consolidate(
        new_node_id: &OsmNodeId,
        nodes: &[&OsmNodeData],
    ) -> Result<OsmNodeData, OsmError> {
        if nodes.is_empty() {
            return Err(OsmError::InternalError(String::from(
                "cannot merge empty vector of nodes",
            )));
        }

        // create a new location from the centroid of the collection of nodes
        let coords = nodes
            .iter()
            .map(|n| Point(Coord::from((n.x, n.y))))
            .collect_vec();
        let mp = MultiPoint::from(coords);
        let centroid = mp.centroid().ok_or_else(|| {
            OsmError::InternalError(String::from("non-empty vector of nodes has no centroid"))
        })?;

        let node_ids = nodes
            .iter()
            .flat_map(|n| {
                let mut ids = n.consolidated_ids.clone();
                ids.push(n.osmid);
                ids
            })
            .collect_vec();

        let highway: Option<String> = collect_attribute(nodes, |n| n.highway.clone());
        let ele: Option<String> = collect_attribute(nodes, |n| n.ele.clone());
        let junction: Option<String> = collect_attribute(nodes, |n| n.junction.clone());
        let railway: Option<String> = collect_attribute(nodes, |n| n.railway.clone());
        let _ref: Option<String> = collect_attribute(nodes, |n| n._ref.clone());

        let result = OsmNodeData {
            osmid: *new_node_id,
            x: centroid.x(),
            y: centroid.y(),
            highway,
            ele,
            junction,
            railway,
            _ref,
            consolidated_ids: node_ids,
        };
        Ok(result)
    }
}

impl Intersects<Geometry<f32>> for OsmNodeData {
    fn intersects(&self, rhs: &Geometry<f32>) -> bool {
        rhs.intersects(&geo::Point::new(self.x, self.y))
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

/// helper function that parses ele values. these should be in meters and contain
/// only numeric strings according to the documentation, and we follow that logic
/// optimistically here by supressing parse failures.
fn parse_ele(ele: &str) -> Option<f64> {
    ele.parse::<i64>()
        .map(|i| i as f64)
        .or_else(|_| ele.parse::<f64>())
        // .map_err(|e| {
        //     format!(
        //         "unable to parse 'ele' value as integer or decimal number: {}",
        //         e
        //     )
        // })
        .ok()
}

/// helper to combine all unique values for a key and concatenate them
/// with whitespace
fn collect_attribute(
    nodes: &[&OsmNodeData],
    op: impl FnMut(&&OsmNodeData) -> Option<String>,
) -> Option<String> {
    let s = nodes
        .iter()
        .flat_map(op)
        .dedup()
        .sorted()
        .join(OsmNodeData::VALUE_DELIMITER);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
