use std::{
    error::Error,
    fs::File,
    io::{BufReader, Read},
};

use flate2::read::GzDecoder;
use geo::{Distance, Euclidean};
use serde::de::DeserializeOwned;

use crate::{
    common::{edge_rtree_entry::EdgeRTreeEntry, modal_metrics::ModalMetricError},
    network_traits::{
        edge_for_modal_metric::EdgeForModalMetric,
        spatial_edge::SpatialEdge,
        vertex_for_modal_metric::{CompactVertexAttrs, VertexForModalMetric},
    },
};

/// Opens a streaming CSV reader supporting both plain `.csv` and gzip-compressed `.csv.gz` files.
pub fn open_csv_reader(filepath: &str) -> Result<csv::Reader<Box<dyn Read>>, Box<dyn Error>> {
    let file =
        File::open(filepath).map_err(|e| format!("failed to open CSV file '{filepath}': {e}"))?;
    let reader: Box<dyn Read> = if filepath.ends_with(".gz") {
        Box::new(GzDecoder::new(BufReader::with_capacity(1 << 20, file)))
    } else {
        Box::new(BufReader::with_capacity(1 << 20, file))
    };
    Ok(csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader))
}

/// Streams `vertices_file` record-by-record into a compact 1-byte-per-vertex lookup table,
/// immediately dropping each full vertex struct (`V`) and its heap allocations.
pub fn load_compact_vertices_streaming<V>(
    vertices_file: &str,
) -> Result<Vec<CompactVertexAttrs>, Box<dyn Error>>
where
    V: VertexForModalMetric + DeserializeOwned,
{
    let mut rdr = open_csv_reader(vertices_file)?;
    let mut vertices = Vec::new();
    for (row_idx, result) in rdr.deserialize::<V>().enumerate() {
        let vertex = result.map_err(|e| {
            format!("failed deserializing vertex at row {row_idx} in '{vertices_file}': {e}")
        })?;
        vertices.push(CompactVertexAttrs::from_vertex(&vertex));
    }
    Ok(vertices)
}

/// Streams `edges_file` record-by-record and appends compact [`EdgeRTreeEntry`] items into `out`,
/// returning the number of edges appended from `edges_file`.
pub fn append_edge_rtree_entries_streaming<E, V>(
    edges_file: &str,
    vertices: &[V],
    out: &mut Vec<EdgeRTreeEntry<E>>,
) -> Result<usize, Box<dyn Error>>
where
    E: SpatialEdge + EdgeForModalMetric + DeserializeOwned,
    V: VertexForModalMetric,
{
    let mut rdr = open_csv_reader(edges_file)?;
    let mut count = 0usize;

    for (row_idx, result) in rdr.deserialize::<E>().enumerate() {
        let edge = result.map_err(|e| {
            format!("failed deserializing edge at row {row_idx} in '{edges_file}': {e}")
        })?;
        let src = edge.src_vertex_id();
        if vertices.get(src).is_none() {
            log::warn!(
                "source vertex {src} not found for edge {} (row {row_idx}); proceeding without source vertex attributes",
                edge.id_str().map( str::to_owned).unwrap_or_else(|| edge.id())
            );
        }

        let entry = EdgeRTreeEntry::from_ref(&edge).ok_or_else(|| {
            let id = edge
                .id_str()
                .map(str::to_owned)
                .unwrap_or_else(|| edge.id());
            ModalMetricError::Other(format!(
                "could not create R-tree entry for edge '{id}' at row {row_idx} in '{edges_file}'"
            ))
        })?;

        out.push(entry);
        count += 1;
    }

    Ok(count)
}

/// Load edges from a CSV or `.csv.gz` file and create R-tree entries for each edge,
/// preserving 1-to-1 row order with the input file.
pub fn load_edge_rtree_entries<E, V>(
    edges_file: &str,
    vertices: &[V],
) -> Result<Vec<EdgeRTreeEntry<E>>, Box<dyn Error>>
where
    E: SpatialEdge + EdgeForModalMetric + DeserializeOwned,
    V: VertexForModalMetric,
{
    let mut edge_entries = Vec::new();
    append_edge_rtree_entries_streaming(edges_file, vertices, &mut edge_entries)?;
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
    let mut sum_distances: f32 = 0.0;
    let mut weighted_speed_num: f32 = 0.0;

    for neighbor in neighboring_edges {
        if let Some(mph) = neighbor.edge.get_traffic_speed_limit() {
            let speed = mph as f32;
            let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
            sum_distances += distance;
            weighted_speed_num += speed * distance;
        }
    }

    if sum_distances == 0.0 {
        return None;
    }

    Some(weighted_speed_num / sum_distances)
}
