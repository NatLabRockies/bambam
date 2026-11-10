use std::error::Error;

use geo::{Distance, Euclidean};
use routee_compass_core::util::fs::read_utils;
use serde::de::DeserializeOwned;

use crate::{
    common::{edge_rtree_entry::EdgeRTreeEntry, modal_metrics::ModalMetricError},
    network_traits::{
        edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge,
        vertex_for_modal_metric::VertexForModalMetric,
    },
};

/// Load edges from a CSV or `.csv.gz` file and create R-tree entries for each edge,
/// preserving 1-to-1 row order with the input file.
pub fn load_edge_rtree_entries<E, V>(
    edges_file: &str,
    vertices: &[V],
) -> Result<Vec<EdgeRTreeEntry<E>>, Box<dyn Error>>
where
    E: SpatialEdge + DeserializeOwned,
    V: VertexForModalMetric,
{
    let edges: Box<[E]> = read_utils::from_csv(&edges_file, true, None, None)?;
    let mut edge_entries = Vec::with_capacity(edges.len());

    for (row_idx, edge) in edges.into_vec().into_iter().enumerate() {
        let src = edge.src_vertex_id();
        if vertices.get(src).is_none() {
            log::warn!(
                "source vertex {src} not found for edge {} (row {row_idx}); proceeding without source vertex attributes",
                edge.id()
            );
        }

        let id = edge.id();
        let entry = EdgeRTreeEntry::new(edge).ok_or_else(|| {
            ModalMetricError::Other(format!(
                "could not create R-tree entry for edge '{id}' at row {row_idx} in '{edges_file}'"
            ))
        })?;

        edge_entries.push(entry);
    }

    Ok(edge_entries)
}

/// Traffic speed limit in MPH, if known.
pub fn traffic_speed_from_maxspeed<E>(entry: &EdgeRTreeEntry<E>) -> Option<f32>
where
    E: EdgeForModalMetric + SpatialEdge,
{
    entry.edge.get_traffic_speed_limit().map(|mph| mph as f32)
}

/// Computes a weighted estimated speed (in MPH) from nearby ways
pub fn estimated_speed_from_neighbors<E>(
    entry: &EdgeRTreeEntry<E>,
    neighboring_edges: &[&EdgeRTreeEntry<E>],
) -> Option<f32>
where
    E: EdgeForModalMetric + SpatialEdge,
{
    let speeds_and_distances: Vec<(f32, f32)> = neighboring_edges
        .iter()
        .filter_map(|neighbor| {
            neighbor.edge.get_traffic_speed_limit().map(|mph| {
                let speed = mph as f32;
                let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
                (speed, distance)
            })
        })
        .collect();

    if speeds_and_distances.is_empty() {
        return None;
    }

    let sum_distances: f32 = speeds_and_distances
        .iter()
        .map(|(_, distance)| *distance)
        .sum();

    if sum_distances == 0.0 {
        return None;
    }

    let weighted_speed: f32 = speeds_and_distances
        .iter()
        .map(|(speed, distance)| speed * distance / sum_distances)
        .sum();

    Some(weighted_speed)
}
