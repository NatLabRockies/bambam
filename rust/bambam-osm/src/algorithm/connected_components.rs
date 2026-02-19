use crate::model::osm::{
    graph::{OsmGraph, OsmNodeId},
    OsmError,
};
use itertools::Itertools;
use kdam::{tqdm, Bar, BarExt};
use std::collections::{HashMap, HashSet, VecDeque};

pub type UndirectedAdjacencyList = HashMap<OsmNodeId, HashSet<OsmNodeId>>;

pub fn to_undirected(graph: &OsmGraph) -> UndirectedAdjacencyList {
    let mut undirected: HashMap<OsmNodeId, HashSet<OsmNodeId>> = HashMap::new();
    let adjacencies_iter = tqdm!(
        graph.connected_node_pair_iterator(false),
        desc = "create undirected graph"
    );
    for (src, dst) in adjacencies_iter {
        add_undirected_edge(src, dst, &mut undirected);
    }

    eprintln!();
    undirected
}

/// helper that creates the relations src->dst, dst->src from a directed edge src->dst
/// guards against self-loops.
fn add_undirected_edge(src: &OsmNodeId, dst: &OsmNodeId, g: &mut UndirectedAdjacencyList) {
    if src == dst {
        return;
    }
    g.entry(*src)
        .and_modify(|h| {
            h.insert(*dst);
        })
        .or_insert(HashSet::from([*dst]));
    g.entry(*dst)
        .and_modify(|h| {
            h.insert(*src);
        })
        .or_insert(HashSet::from([*src]));
}

/// runs a synchronous weakly-connected components algorithm over the directed graph.
///
/// # Arguments
///
/// * `fwd` - forward traversal segments, the "out-edges" of the nodes
/// * `rev` - reverse traversal segments, the "in-edges" of the nodes
/// * `nodes` - the graph nodes included to find components.
///  this can either be the complete set or a subset.
///
/// # Result
///
/// a vector of each found component as a node list
pub fn weakly_connected_components(
    graph: &OsmGraph,
    nodes: &[OsmNodeId],
) -> Result<Vec<Vec<OsmNodeId>>, OsmError> {
    let undirected = to_undirected(graph);

    let n_unassigned = nodes.len();
    let mut assigned: HashSet<OsmNodeId> = HashSet::new();
    let mut solution: Vec<Vec<OsmNodeId>> = vec![];
    let mut bar = Bar::builder()
        .total(n_unassigned)
        .desc("weakly connected components search")
        .build()
        .map_err(OsmError::InternalError)?;

    // create a new component any time we find an unattached node
    for node_id in nodes.iter() {
        if !assigned.contains(node_id) {
            let cluster = bfs_undirected(node_id, &undirected)?;
            for cluster_node_id in cluster.iter() {
                assigned.insert(*cluster_node_id);
            }
            solution.push(cluster);
        }
        let _ = bar.update(1);
    }
    eprintln!();
    log::info!("found {} weakly-connected components", solution.len());
    Ok(solution)
}

/// runs an undirected breadth-first search from some source
/// to find all weakly-connected nodes.
pub fn bfs_undirected(
    source: &OsmNodeId,
    graph: &UndirectedAdjacencyList,
) -> Result<Vec<OsmNodeId>, OsmError> {
    // the solution set, beginning with the source node
    let mut visited: HashSet<OsmNodeId> = HashSet::from([*source]);

    // initialize the search frontier with the neighbors of source
    let mut frontier: VecDeque<OsmNodeId> = VecDeque::new();
    for n in graph.get(source).into_iter().flatten() {
        frontier.push_back(*n);
    }

    let mut bar = Bar::builder()
        .desc("connected components")
        .build()
        .map_err(|e| OsmError::InternalError(e.to_string()))?;

    // recurse through graph until all weakly connected neighbors are found
    while let Some(next_id) = frontier.pop_front() {
        if !visited.contains(&next_id) {
            visited.insert(next_id);
            if visited.len() == graph.len() {
                // all nodes have been visited
                return Ok(visited.into_iter().collect_vec());
            }
            for n in graph.get(&next_id).into_iter().flatten() {
                if !visited.contains(n) {
                    frontier.push_back(*n);
                }
            }
        }

        let _ = bar.update(1);
    }

    Ok(visited.into_iter().collect_vec())
}

#[cfg(test)]
mod tests {
    use crate::model::osm::graph::{OsmGraph, OsmNodeData, OsmNodeId, OsmWayData, OsmWayId};
    use std::collections::HashMap;

    #[test]
    fn test_bfs_circle_with_dot() {
        let source = OsmNodeId(0);
        let n_connected_nodes: usize = 20;

        let mut nodes = create_nodes_in_circle(n_connected_nodes);
        // set up bogie, a lonely and unattached node
        nodes.insert(OsmNodeId(999), OsmNodeData::default());

        let mut ways: HashMap<OsmWayId, OsmWayData> = HashMap::new();
        let n_iters = (n_connected_nodes - 1) as i64;
        for i in 0..n_iters {
            let reversed = i % 2 == 0;
            let src = if reversed {
                OsmNodeId(i + 1)
            } else {
                OsmNodeId(i)
            };
            let dst = if reversed {
                OsmNodeId(i)
            } else {
                OsmNodeId(i + 1)
            };
            let way_id = OsmWayId(i);
            let mut way = OsmWayData::default();
            way.osmid = way_id;
            way.nodes = vec![src, dst];

            ways.insert(way.osmid, way);
        }

        // we expect two components, one for the circle, and one for the dot
        let graph = OsmGraph::new(nodes, ways).unwrap();
        let undirected_graph = super::to_undirected(&graph);
        let result = super::bfs_undirected(&source, &undirected_graph).unwrap();
        assert_eq!(result.len(), n_connected_nodes);
    }

    #[test]
    fn test_bfs_complete_graph_5() {
        let source = OsmNodeId(0);
        let n_connected_nodes: usize = 5;
        let nodes = create_nodes_in_circle(n_connected_nodes);
        let mut ways: HashMap<OsmWayId, OsmWayData> = HashMap::new();

        let n_iters = n_connected_nodes as i64;
        let mut way_id = 0;
        for i in 0..n_iters {
            let src = OsmNodeId(i);
            for j in 0..n_iters {
                if i == j {
                    continue;
                }
                let dst = OsmNodeId(j);
                let this_way_id = OsmWayId(way_id);
                let mut way = OsmWayData::default();
                way.osmid = this_way_id;
                way.nodes = vec![src, dst];
                ways.insert(way.osmid, way);
                way_id += 1;
            }
        }
        let graph = OsmGraph::new(nodes, ways).unwrap();
        let undirected_graph = super::to_undirected(&graph);

        eprintln!(
            "{}",
            serde_json::to_string_pretty(&undirected_graph).unwrap()
        );

        let result = super::bfs_undirected(&source, &undirected_graph).unwrap();
        assert_eq!(result.len(), n_connected_nodes);
        eprintln!("{result:?}");
    }

    // helper function that creates nodes with coordinates evenly spaced in a circle
    fn create_nodes_in_circle(n_connected_nodes: usize) -> HashMap<OsmNodeId, OsmNodeData> {
        (0..n_connected_nodes)
            .map(|n| {
                let node_id = OsmNodeId(n.try_into().unwrap());
                let mut node = OsmNodeData::default();
                node.osmid = node_id;
                let angle = 2.0 * std::f32::consts::PI * (n as f32) / (n_connected_nodes as f32);
                let x = angle.cos();
                let y = angle.sin();
                node.x = x;
                node.y = y;
                (node_id, node)
            })
            .collect()
    }
}
