use crate::common::edge_rtree_entry::EdgeRTreeEntry;
use crate::common::modal_metrics::{ModalMetric, ModalMetricError, ModalMetricValue};
use crate::common::ops::{
    append_edge_rtree_entries_streaming, load_compact_vertices_streaming, open_csv_reader,
};
use crate::network_traits::{
    edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge,
    vertex_for_modal_metric::VertexForModalMetric,
};
use flate2::{write::GzEncoder, Compression};
use kdam::{BarBuilder, BarExt};
use rayon::prelude::*;
use rstar::RTree;
use serde::de::DeserializeOwned;
use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
};

/// Number of target edges processed and flushed per streaming batch.
const STREAM_BATCH_SIZE: usize = 250_000;

/// Bulk compute a specific modal metric for all edges in a network by taking in a
/// `vertices-complete.csv[.gz]` and `edges-complete.csv[.gz]`.
pub fn bulk_compute_modal_metric<E, V>(
    metric: ModalMetric,
    edges_file: &str,
    vertices_file: &str,
    output_file: &str,
) -> Result<(), Box<dyn Error>>
where
    E: SpatialEdge + EdgeForModalMetric + DeserializeOwned + Clone + Send + Sync,
    V: VertexForModalMetric + DeserializeOwned + Send + Sync,
{
    bulk_compute_modal_metric_with_context::<E, V>(
        metric,
        edges_file,
        vertices_file,
        &[],
        output_file,
    )
}

/// Bulk compute a specific modal metric for all edges in `edges_file`, optionally populating
/// the spatial R-tree with additional `context_edges_files` (such as adjacent `drive` or `bike`
/// mode edge lists in a multi-modal OMF/OSM network directory).
pub fn bulk_compute_modal_metric_with_context<E, V>(
    metric: ModalMetric,
    edges_file: &str,
    vertices_file: &str,
    context_edges_files: &[String],
    output_file: &str,
) -> Result<(), Box<dyn Error>>
where
    E: SpatialEdge + EdgeForModalMetric + DeserializeOwned + Clone + Send + Sync,
    V: VertexForModalMetric + DeserializeOwned + Send + Sync,
{
    log::info!(
        "\nLoading files for {:?} modal metric computation.\n",
        metric
    );
    log::info!("Reading:\n\t- vertex set @ {vertices_file}\n\t- edge set @ {edges_file}\n");

    // Stream vertices into a compact 1-byte-per-vertex array and drop full vertex structs.
    let vertices = load_compact_vertices_streaming::<V>(vertices_file)?;

    // Stream target + context edges directly into a single compact R-tree entry vector.
    let mut rtree_entries: Vec<EdgeRTreeEntry<E>> = Vec::new();
    let target_edge_count =
        append_edge_rtree_entries_streaming::<E, _>(edges_file, &vertices, &mut rtree_entries)?;

    for ctx_file in context_edges_files {
        if ctx_file != edges_file {
            log::info!("Reading context edge set for R-tree @ {ctx_file}");
            append_edge_rtree_entries_streaming::<E, _>(ctx_file, &vertices, &mut rtree_entries)?;
        }
    }
    log::info!(
        "Edges ({target_edge_count} target, {} total R-tree entries) and vertices ({}) read successfully.\n",
        rtree_entries.len(),
        vertices.len()
    );

    // Build an R-tree consuming the compact entry vector in place.
    let rtree = RTree::bulk_load(rtree_entries);

    let mut bar = BarBuilder::default()
        .desc(format!(
            "Computing {:?} for edges in the road network",
            metric
        ))
        .total(target_edge_count)
        .build()?;

    // Open output writer up front so results can be streamed out batch-by-batch.
    let file = File::create(output_file)?;
    let mut writer: Box<dyn Write> = if output_file.ends_with(".gz") {
        Box::new(BufWriter::with_capacity(
            1 << 20,
            GzEncoder::new(file, Compression::default()),
        ))
    } else {
        Box::new(BufWriter::with_capacity(1 << 20, file))
    };
    metric.write_csv_header(&mut writer)?;

    // Stream target edges in bounded batches, compute modal metric in parallel, and write immediately.
    let mut rdr = open_csv_reader(edges_file)?;
    let mut edge_iter = rdr.deserialize::<E>().enumerate();
    let mut batch: Vec<EdgeRTreeEntry<E>> =
        Vec::with_capacity(target_edge_count.min(STREAM_BATCH_SIZE));

    loop {
        batch.clear();
        for (row_idx, result) in (&mut edge_iter).take(STREAM_BATCH_SIZE) {
            let edge = result.map_err(|e| {
                format!("failed deserializing edge at row {row_idx} in '{edges_file}': {e}")
            })?;
            let entry = EdgeRTreeEntry::from_ref(&edge).ok_or_else(|| {
                let id = edge
                    .id_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| edge.id());
                ModalMetricError::Other(format!(
                    "could not create R-tree entry for edge '{id}' at row {row_idx} in '{edges_file}'"
                ))
            })?;
            batch.push(entry);
        }

        if batch.is_empty() {
            break;
        }

        let batch_values: Vec<ModalMetricValue> = batch
            .par_iter()
            .map(|edge_entry| {
                let src_vertex = vertices.get(edge_entry.edge.src_vertex_id());
                metric.compute_metric(&rtree, edge_entry, src_vertex)
            })
            .collect::<Result<Vec<ModalMetricValue>, ModalMetricError>>()?;

        for value in &batch_values {
            value.write_csv_row(&mut writer)?;
        }
        let _ = bar.update(batch.len());
    }

    eprintln!();
    writer.flush()?;

    log::info!(
        "\n\n{:?} values computed successfully.\n\nOutput file saved @ {output_file}.",
        metric
    );
    Ok(())
}
