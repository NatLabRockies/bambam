use std::collections::{HashSet, VecDeque};

use geo::{line_string, Haversine, Length};
use indexmap::IndexMap;
use kdam::{tqdm, BarExt};
use routee_compass_core::model::{
    network::{Edge, EdgeId, EdgeList, EdgeListId, Vertex, VertexId},
    unit::DistanceUnit,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uom::si::f64::Length as uom_length;

use crate::error::NetworkImportError;

/// Algorithm used to identify disconnected sub-sections of the road network graph
/// (referred to here as 'islands') and remove them.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentsAlgorithmType {
    /// Strongly Connected Components (SCC).
    Scc,
    #[default]
    /// Weakly Connected Components (WCC).
    Wcc,
    /// Iterative Leaf Pruning.
    IterativeLeafPruning,
}

/// runs a connected components algorithm to identify edges
/// that can be removed from the graph.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct IslandDetectionAlgorithm {
    pub min_distance: f64,
    #[schemars(with = "String")]
    pub distance_unit: DistanceUnit,
    #[serde(default)]
    pub algorithm_type: ComponentsAlgorithmType,
}

impl IslandDetectionAlgorithm {
    /// run the algorithm, producing the list of edges that are part of
    /// mobility islands which we want to remove from the algorithm.
    pub fn run(
        &self,
        edge_lists: &[&EdgeList],
        vertices: &[Vertex],
    ) -> Result<Vec<(EdgeListId, EdgeId)>, NetworkImportError> {
        let mut result = vec![];

        for edge_list in edge_lists.iter() {
            match self.algorithm_type {
                ComponentsAlgorithmType::Scc => {
                    let remove_list =
                        kosaraju_scc(edge_list, vertices, self.min_distance, self.distance_unit)?;
                    result.extend(remove_list);
                }
                ComponentsAlgorithmType::Wcc => {
                    let remove_list =
                        wcc_components(edge_list, vertices, self.min_distance, self.distance_unit)?;
                    result.extend(remove_list);
                }
                ComponentsAlgorithmType::IterativeLeafPruning => {
                    let remove_list = iterative_leaf_pruning(
                        edge_list,
                        vertices,
                        self.min_distance,
                        self.distance_unit,
                    )?;
                    result.extend(remove_list);
                }
            }
        }
        Ok(result)
    }
}

pub type DenseAdjacencyList = Box<[IndexMap<(EdgeListId, EdgeId), VertexId>]>;

pub fn kosaraju_scc(
    edge_list: &EdgeList,
    vertices: &[Vertex],
    distance_threshold: f64,
    distance_threshold_unit: DistanceUnit,
) -> Result<Vec<(EdgeListId, EdgeId)>, NetworkImportError> {
    let forward_adjacency: DenseAdjacencyList = build_adjacency(edge_list, vertices.len(), true)
        .map_err(|s| {
            NetworkImportError::InternalError(format!(
                "failed to compute adjacency matrix for island detection algorithm: {s}"
            ))
        })?;
    let backward_adjacency: DenseAdjacencyList = build_adjacency(edge_list, vertices.len(), false)
        .map_err(|s| {
            NetworkImportError::InternalError(format!(
                "failed to compute adjacency matrix for island detection algorithm: {s}"
            ))
        })?;

    let mut pb = tqdm!(
        total = edge_list.len(),
        desc = "computing components - scanning edges"
    );

    let mut visited_forward = HashSet::<(EdgeListId, EdgeId)>::new();
    let mut post_order = Vec::<(EdgeListId, EdgeId)>::new();

    for start_edge in edge_list.edges() {
        let edge_key = (start_edge.edge_list_id, start_edge.edge_id);
        if visited_forward.contains(&edge_key) {
            continue;
        }

        let mut stack = vec![(edge_key, false)];
        while let Some((curr, is_post)) = stack.pop() {
            if is_post {
                post_order.push(curr);
                continue;
            }

            if visited_forward.contains(&curr) {
                continue;
            }
            visited_forward.insert(curr);
            stack.push((curr, true));

            let curr_edge = edge_list.0[curr.1 .0];
            let outward_edges: Vec<&(EdgeListId, EdgeId)> = forward_adjacency
                [curr_edge.dst_vertex_id.0]
                .keys()
                .collect();

            for &next_edge_key in outward_edges {
                if !visited_forward.contains(&next_edge_key) {
                    stack.push((next_edge_key, false));
                }
            }
        }
    }

    let mut visited_backward = HashSet::<(EdgeListId, EdgeId)>::new();
    let mut flagged = Vec::<(EdgeListId, EdgeId)>::new();

    for &start_edge_key in post_order.iter().rev() {
        if visited_backward.contains(&start_edge_key) {
            continue;
        }

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut scc = Vec::<(EdgeListId, EdgeId)>::new();

        let mut queue = VecDeque::<(EdgeListId, EdgeId)>::new();
        queue.push_back(start_edge_key);
        visited_backward.insert(start_edge_key);

        while let Some(curr) = queue.pop_front() {
            scc.push(curr);
            let current_edge = edge_list.0[curr.1 .0];

            let src_vertex = vertices[current_edge.src_vertex_id.0];
            let dst_vertex = vertices[current_edge.dst_vertex_id.0];
            min_x = min_x.min(src_vertex.x()).min(dst_vertex.x());
            max_x = max_x.max(src_vertex.x()).max(dst_vertex.x());
            min_y = min_y.min(src_vertex.y()).min(dst_vertex.y());
            max_y = max_y.max(src_vertex.y()).max(dst_vertex.y());

            let inward_edges: Vec<&(EdgeListId, EdgeId)> = backward_adjacency
                [current_edge.src_vertex_id.0]
                .keys()
                .collect();

            for &next_edge_key in inward_edges {
                if !visited_backward.contains(&next_edge_key) {
                    visited_backward.insert(next_edge_key);
                    queue.push_back(next_edge_key);
                }
            }

            if let Err(e) = pb.update(1) {
                log::warn!("error during update of progress bar: {e}")
            };
        }

        let component_diagonal_meters =
            Haversine.length(&line_string![(min_x, min_y).into(), (max_x, max_y).into()]);
        let diameter_uom =
            uom_length::new::<uom::si::length::meter>(component_diagonal_meters as f64);

        if diameter_uom < distance_threshold_unit.to_uom(distance_threshold) {
            flagged.append(&mut scc);
        }
    }

    eprintln!();
    Ok(flagged)
}

pub fn wcc_components(
    edge_list: &EdgeList,
    vertices: &[Vertex],
    distance_threshold: f64,
    distance_threshold_unit: DistanceUnit,
) -> Result<Vec<(EdgeListId, EdgeId)>, NetworkImportError> {
    let forward_adjacency: DenseAdjacencyList = build_adjacency(edge_list, vertices.len(), true)
        .map_err(|s| {
            NetworkImportError::InternalError(format!(
                "failed to compute forward adjacency matrix for wcc: {s}"
            ))
        })?;
    let backward_adjacency: DenseAdjacencyList = build_adjacency(edge_list, vertices.len(), false)
        .map_err(|s| {
            NetworkImportError::InternalError(format!(
                "failed to compute backward adjacency matrix for wcc: {s}"
            ))
        })?;
    let mut pb = tqdm!(total = edge_list.len(), desc = "computing wcc components");

    let mut visited = HashSet::<(EdgeListId, EdgeId)>::new();
    let mut flagged = Vec::<(EdgeListId, EdgeId)>::new();

    for start_edge in edge_list.edges() {
        let start_edge_key = (start_edge.edge_list_id, start_edge.edge_id);
        if visited.contains(&start_edge_key) {
            continue;
        }

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut component = Vec::<(EdgeListId, EdgeId)>::new();

        let mut queue = VecDeque::<(EdgeListId, EdgeId)>::new();
        queue.push_back(start_edge_key);
        visited.insert(start_edge_key);

        while let Some(curr) = queue.pop_front() {
            component.push(curr);
            let current_edge = edge_list.0[curr.1 .0];

            let src_vertex = vertices[current_edge.src_vertex_id.0];
            let dst_vertex = vertices[current_edge.dst_vertex_id.0];
            min_x = min_x.min(src_vertex.x()).min(dst_vertex.x());
            max_x = max_x.max(src_vertex.x()).max(dst_vertex.x());
            min_y = min_y.min(src_vertex.y()).min(dst_vertex.y());
            max_y = max_y.max(src_vertex.y()).max(dst_vertex.y());

            for (&next_edge_key, _) in forward_adjacency[current_edge.dst_vertex_id.0].iter() {
                if !visited.contains(&next_edge_key) {
                    visited.insert(next_edge_key);
                    queue.push_back(next_edge_key);
                }
            }
            for (&next_edge_key, _) in backward_adjacency[current_edge.dst_vertex_id.0].iter() {
                if !visited.contains(&next_edge_key) {
                    visited.insert(next_edge_key);
                    queue.push_back(next_edge_key);
                }
            }
            for (&next_edge_key, _) in forward_adjacency[current_edge.src_vertex_id.0].iter() {
                if !visited.contains(&next_edge_key) {
                    visited.insert(next_edge_key);
                    queue.push_back(next_edge_key);
                }
            }
            for (&next_edge_key, _) in backward_adjacency[current_edge.src_vertex_id.0].iter() {
                if !visited.contains(&next_edge_key) {
                    visited.insert(next_edge_key);
                    queue.push_back(next_edge_key);
                }
            }

            let _ = pb.update(1);
        }

        let component_diagonal_meters =
            Haversine.length(&line_string![(min_x, min_y).into(), (max_x, max_y).into()]);
        let diameter_uom =
            uom_length::new::<uom::si::length::meter>(component_diagonal_meters as f64);

        if diameter_uom < distance_threshold_unit.to_uom(distance_threshold) {
            flagged.append(&mut component);
        }
    }

    eprintln!();
    Ok(flagged)
}

pub fn iterative_leaf_pruning(
    edge_list: &EdgeList,
    vertices: &[Vertex],
    _distance_threshold: f64,
    _distance_threshold_unit: DistanceUnit,
) -> Result<Vec<(EdgeListId, EdgeId)>, NetworkImportError> {
    let forward_adjacency: DenseAdjacencyList = build_adjacency(edge_list, vertices.len(), true)
        .map_err(|s| {
            NetworkImportError::InternalError(format!(
                "failed to compute forward adjacency matrix for leaf pruning: {s}"
            ))
        })?;
    let backward_adjacency: DenseAdjacencyList = build_adjacency(edge_list, vertices.len(), false)
        .map_err(|s| {
            NetworkImportError::InternalError(format!(
                "failed to compute backward adjacency matrix for leaf pruning: {s}"
            ))
        })?;

    let mut pb = tqdm!(total = edge_list.len(), desc = "computing leaf pruning");

    let mut out_degree = vec![0; vertices.len()];
    let mut in_degree = vec![0; vertices.len()];
    let mut active_edges = HashSet::<(EdgeListId, EdgeId)>::new();

    for edge in edge_list.edges() {
        out_degree[edge.src_vertex_id.0] += 1;
        in_degree[edge.dst_vertex_id.0] += 1;
        active_edges.insert((edge.edge_list_id, edge.edge_id));
    }

    let mut queue = VecDeque::<VertexId>::new();
    for v in 0..vertices.len() {
        if (in_degree[v] > 0 && out_degree[v] == 0) || (in_degree[v] == 0 && out_degree[v] > 0) {
            queue.push_back(VertexId(v));
        }
    }

    let mut flagged = Vec::<(EdgeListId, EdgeId)>::new();

    while let Some(v) = queue.pop_front() {
        if in_degree[v.0] > 0 && out_degree[v.0] == 0 {
            for (&edge_key, _) in backward_adjacency[v.0].iter() {
                if active_edges.remove(&edge_key) {
                    flagged.push(edge_key);
                    let _ = pb.update(1);
                    let e_info = edge_list.0[edge_key.1 .0];
                    let src = e_info.src_vertex_id;
                    out_degree[src.0] -= 1;
                    if (in_degree[src.0] > 0 && out_degree[src.0] == 0)
                        || (in_degree[src.0] == 0 && out_degree[src.0] > 0)
                    {
                        queue.push_back(src);
                    }
                    in_degree[v.0] -= 1;
                }
            }
        } else if in_degree[v.0] == 0 && out_degree[v.0] > 0 {
            for (&edge_key, _) in forward_adjacency[v.0].iter() {
                if active_edges.remove(&edge_key) {
                    flagged.push(edge_key);
                    let _ = pb.update(1);
                    let e_info = edge_list.0[edge_key.1 .0];
                    let dst = e_info.dst_vertex_id;
                    in_degree[dst.0] -= 1;
                    if (in_degree[dst.0] > 0 && out_degree[dst.0] == 0)
                        || (in_degree[dst.0] == 0 && out_degree[dst.0] > 0)
                    {
                        queue.push_back(dst);
                    }
                    out_degree[v.0] -= 1;
                }
            }
        }
    }

    let _ = pb.update_to(edge_list.len());
    eprintln!();

    Ok(flagged)
}

fn build_adjacency(
    edge_list: &EdgeList,
    n_vertices: usize,
    forward: bool,
) -> Result<DenseAdjacencyList, String> {
    let build_adjacencies_iter = tqdm!(
        edge_list.edges(),
        desc = "building adjacencies",
        total = edge_list.len()
    );

    let mut out_adjacency = vec![IndexMap::<(EdgeListId, EdgeId), VertexId>::new(); n_vertices];
    for edge in build_adjacencies_iter {
        append_to_adjacency(&mut out_adjacency, edge, forward)?;
    }

    Ok(out_adjacency.into_boxed_slice())
}

fn append_to_adjacency(
    adjacency: &mut [IndexMap<(EdgeListId, EdgeId), VertexId>],
    edge: &Edge,
    forward: bool,
) -> Result<(), String> {
    let src_vertex = if forward {
        edge.src_vertex_id
    } else {
        edge.dst_vertex_id
    };

    match adjacency.get_mut(src_vertex.0) {
        None => {
            let direction = if forward { "forward" } else { "reverse" };
            Err(format!(
                "vertex {} not found in {} adjacencies for edge list, edge: {}, {}",
                src_vertex.0, direction, edge.edge_list_id.0, edge.edge_id.0
            ))
        }
        Some(out_links) => {
            let target_vertex = if forward {
                edge.dst_vertex_id
            } else {
                edge.src_vertex_id
            };
            out_links.insert((edge.edge_list_id, edge.edge_id), target_vertex);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use routee_compass_core::model::network::{Edge, EdgeList, Vertex};
    use uom::si::f64::Length;

    #[test]
    fn test_wcc_vs_kosaraju_long_one_way_arterial() {
        let base_lat = 39.7392;
        let base_lon = -104.9903;
        let offset = 0.001;

        let vertices = vec![
            Vertex::new(0, base_lon, base_lat),
            Vertex::new(1, base_lon + offset, base_lat),
            Vertex::new(2, base_lon + 2.0 * offset, base_lat),
            Vertex::new(3, base_lon + 3.0 * offset, base_lat),
        ];

        let edges = vec![
            Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(87.0)),
            Edge::new(0, 1, 1, 2, Length::new::<uom::si::length::meter>(87.0)),
            Edge::new(0, 2, 2, 3, Length::new::<uom::si::length::meter>(87.0)),
        ];
        let edge_list = EdgeList(edges.into_boxed_slice());

        let distance_threshold = 100.0;

        let flagged_wcc = wcc_components(
            &edge_list,
            &vertices,
            distance_threshold,
            DistanceUnit::Meters,
        )
        .unwrap();
        assert!(flagged_wcc.is_empty());

        let flagged_scc = kosaraju_scc(
            &edge_list,
            &vertices,
            distance_threshold,
            DistanceUnit::Meters,
        )
        .unwrap();
        assert_eq!(flagged_scc.len(), 3);
    }

    #[test]
    fn test_wcc_tiny_acyclic_island() {
        let base_lat = 39.7392;
        let base_lon = -104.9903;
        let offset = 0.0005;

        let vertices = vec![
            Vertex::new(0, base_lon, base_lat),
            Vertex::new(1, base_lon + offset, base_lat),
        ];

        let edges = vec![Edge::new(
            0,
            0,
            0,
            1,
            Length::new::<uom::si::length::meter>(43.0),
        )];
        let edge_list = EdgeList(edges.into_boxed_slice());

        let distance_threshold = 100.0;

        let flagged_wcc = wcc_components(
            &edge_list,
            &vertices,
            distance_threshold,
            DistanceUnit::Meters,
        )
        .unwrap();
        assert_eq!(flagged_wcc.len(), 1);
    }

    #[test]
    fn test_iterative_leaf_pruning_lollipop() {
        let vertices = vec![
            Vertex::new(0, 0.0, 0.0),
            Vertex::new(1, 0.0, 0.0),
            Vertex::new(2, 0.0, 0.0),
            Vertex::new(3, 0.0, 0.0),
            Vertex::new(4, 0.0, 0.0),
        ];

        let edges = vec![
            Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(10.0)),
            Edge::new(0, 1, 1, 2, Length::new::<uom::si::length::meter>(10.0)),
            Edge::new(0, 2, 2, 0, Length::new::<uom::si::length::meter>(10.0)),
            Edge::new(0, 3, 2, 3, Length::new::<uom::si::length::meter>(10.0)),
            Edge::new(0, 4, 3, 4, Length::new::<uom::si::length::meter>(10.0)),
        ];
        let edge_list = EdgeList(edges.into_boxed_slice());

        let flagged =
            iterative_leaf_pruning(&edge_list, &vertices, 100.0, DistanceUnit::Meters).unwrap();
        assert_eq!(flagged.len(), 2);

        let flagged_edges: std::collections::HashSet<_> =
            flagged.into_iter().map(|e| e.1 .0).collect();
        assert!(flagged_edges.contains(&3));
        assert!(flagged_edges.contains(&4));
    }
}
