use std::collections::HashSet;

use super::{
    CompassIndex, HashMap, Itertools, OsmError, OsmGraph, OsmNodeDataSerializable, OsmNodeId,
    OsmNodesSerializable, OsmWayDataSerializable, OsmWaysSerializable, Vertex, VertexLookup,
};
use crate::model::osm::graph::{
    osm_way_data_serializable::create_linestring_for_od_path, OsmNodeData, OsmWayData,
};
use geo::Convert;
use geozero::ToWkt;
use kdam::tqdm;

pub struct OsmGraphVectorized {
    /// the collection of OSM nodes associated via their OSMID
    pub nodes: OsmNodesSerializable,
    /// just a list of OSM ways in an arbitrary order. these are unique by OSMID but
    /// not guaranteed to be unique by source and destination node (i.e., multigraph).
    pub ways: OsmWaysSerializable,
    /// for each OsmNodeId, the vertex index
    pub vertex_lookup: VertexLookup,
    /// loaded and simplified/consolidated graph dataset
    pub reference_graph: OsmGraph,
}

impl OsmGraphVectorized {
    /// vectorizes an [`OsmGraph`] such that the position of each node and way in each vector
    /// (their index) becomes their respective VectorId/EdgeId.
    pub fn new(
        graph: OsmGraph,
        ignore_serialization_errors: bool,
    ) -> Result<OsmGraphVectorized, OsmError> {
        // create vertex_ids, serializable nodes and vertex lookup (one-pass)
        let mut nodes: OsmNodesSerializable = Vec::with_capacity(graph.n_connected_nodes());
        let mut vertex_lookup: HashMap<OsmNodeId, (CompassIndex, Vertex)> = HashMap::new();
        let node_iter = tqdm!(
            graph.connected_node_data_iterator(true).enumerate(),
            total = graph.n_connected_nodes(),
            desc = "osm nodes to compass vertices"
        );
        for (vertex_id, result) in node_iter {
            let node = result?;
            let node_ser = OsmNodeDataSerializable::from(node);
            nodes.insert(vertex_id, node_ser);

            let vertex = Vertex::new(vertex_id, node.x, node.y);
            vertex_lookup.insert(node.osmid, (vertex_id, vertex));
        }
        eprintln!();

        // create edge_ids and serializable ways
        let triplet_iter = tqdm!(
            graph.connected_multiedge_way_triplet_iterator(true),
            total = graph.n_connected_ways(),
            desc = "osm ways to compass edges"
        );
        let mut ways: OsmWaysSerializable = vec![];
        for (idx, traj_result) in triplet_iter.enumerate() {
            match traj_result {
                Ok(None) => {}
                Ok(Some(traj)) if traj.is_empty() => {
                    return Err(OsmError::InternalError(format!(
                        "way with EdgeId {idx} has no trajectories"
                    )))
                }
                Ok(Some(traj)) if invalid_linestring(&traj, &graph) => {
                    let linestring = debug_linestring(&traj, &graph);
                    log::warn!("connected ways triplet iterator provided invalid trajectory with linestring: '{linestring}'");
                }
                Ok(Some(traj)) => match OsmWayDataSerializable::new(traj, &graph, &vertex_lookup) {
                    Ok(way) => ways.push(way),
                    Err(_) if ignore_serialization_errors => {}
                    Err(e) => return Err(e),
                },
                Err(e) => return Err(OsmError::GraphModificationError(e.to_string())),
            }
        }
        eprintln!();

        let result = OsmGraphVectorized {
            nodes,
            ways,
            vertex_lookup,
            reference_graph: graph,
        };
        Ok(result)
    }
}

fn invalid_linestring(
    traj: &[(&OsmNodeData, &OsmWayData, &OsmNodeData)],
    graph: &OsmGraph,
) -> bool {
    let nodes_connected = traj
        .iter()
        .flat_map(|(_, e, _)| e.nodes.clone())
        .collect::<HashSet<_>>();
    let mut coords = nodes_connected
        .iter()
        .flat_map(|n| graph.get_node_data(n).ok())
        .map(|n| n.get_point())
        .collect_vec();
    coords.dedup_by(|a, b| {
        ((a.x() * 10000.0f32) as i64) == ((b.x() * 10000.0f32) as i64)
            && ((a.y() * 10000.0f32) as i64) == ((b.y() * 10000.0f32) as i64)
    });
    nodes_connected.len() < 2
}

fn debug_linestring(
    traj: &[(&OsmNodeData, &OsmWayData, &OsmNodeData)],
    graph: &OsmGraph,
) -> String {
    match traj.first() {
        None => "invalid: empty linestring".to_string(),
        Some((src, way, dst)) => create_linestring_for_od_path(&src.osmid, &dst.osmid, way, graph)
            .map(|l| {
                let l_f64: geo::LineString<f64> = l.convert();
                geo::Geometry::from(l_f64).to_wkt().unwrap_or_default()
            })
            .unwrap_or_else(|_| "invalid: unable to construct linestring".to_string()),
    }
}
