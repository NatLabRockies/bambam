use crate::common::modal_metrics::{ModalMetric, ModalMetricError, ModalMetricValue};
use crate::common::ops::load_edge_rtree_entries;
use crate::network_traits::{
    edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge,
    vertex_for_modal_metric::VertexForModalMetric,
};
use flate2::{write::GzEncoder, Compression};
use kdam::{Bar, BarBuilder, BarExt};
use rayon::prelude::*;
use routee_compass_core::util::fs::read_utils;
use rstar::RTree;
use serde::de::DeserializeOwned;
use std::sync::{Arc, Mutex};
use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
};

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

    // load vertices and target edges.
    let vertices: Box<[V]> = read_utils::from_csv(&vertices_file, true, None, None)?;
    let edge_rtree_entries = load_edge_rtree_entries::<E, V>(edges_file, &vertices)?;

    let mut rtree_entries = edge_rtree_entries.clone();
    for ctx_file in context_edges_files {
        if ctx_file != edges_file {
            log::info!("Reading context edge set for R-tree @ {ctx_file}");
            let ctx_entries = load_edge_rtree_entries::<E, V>(ctx_file, &vertices)?;
            rtree_entries.extend(ctx_entries);
        }
    }
    log::info!("Edges and vertices read successfully.\n");

    // build an RTree with target + context edge entries.
    let rtree = RTree::bulk_load(rtree_entries);

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .desc(format!(
                "Computing {:?} for edges in the road network",
                metric
            ))
            .total(edge_rtree_entries.len())
            .build()?,
    ));

    // compute the modal metric for each target edge in parallel via par_iter
    let values: Vec<ModalMetricValue> = edge_rtree_entries
        .par_iter()
        .map(|edge_entry| {
            let src_vertex = vertices.get(edge_entry.edge.src_vertex_id());

            let result = metric.compute_metric(&rtree, edge_entry, src_vertex)?;

            if let Ok(mut bar) = bar.lock() {
                let _ = bar.update(1);
            }

            Ok(result)
        })
        .collect::<Result<Vec<ModalMetricValue>, ModalMetricError>>()?;

    eprintln!();

    // write to file (supports both plain .csv and compressed .csv.gz)
    let file = File::create(output_file)?;
    let mut writer: Box<dyn Write> = if output_file.ends_with(".gz") {
        Box::new(BufWriter::new(GzEncoder::new(file, Compression::default())))
    } else {
        Box::new(BufWriter::new(file))
    };

    metric.write_csv_header(&mut writer)?;
    for value in &values {
        value.write_csv_row(&mut writer)?;
    }
    writer.flush()?;

    log::info!(
        "\n\n{:?} values computed successfully.\n\nOutput file saved @ {output_file}.",
        metric
    );
    Ok(())
}
