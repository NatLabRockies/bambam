use crate::common::modal_metrics::{ModalMetric, ModalMetricError, ModalMetricValue};
use crate::common::ops::load_edge_rtree_entries;
use crate::network_traits::{
    edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge,
    vertex_for_modal_metric::VertexForModalMetric,
};
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

/// Bulk compute a specific modal metric for all ways in an OSM network by taking in a vertices-complete.csv
/// and edges-complete.csv.
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
    log::info!(
        "\nLoading files for {:?} modal metric computation.\n",
        metric
    );
    log::info!("Reading:\n\t- vertex set @ {vertices_file}\n\t- edge set @ {edges_file}\n");

    // load vertices and edges.
    let vertices: Box<[V]> = read_utils::from_csv(&vertices_file, true, None, None)?;
    let edge_rtree_entries = load_edge_rtree_entries::<E, V>(edges_file, &vertices)?;
    log::info!("Edges and vertices read successfully.\n");

    // build an RTree with the edge entries.
    let rtree = RTree::bulk_load(edge_rtree_entries.clone());

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .desc(format!(
                "Computing {:?} for edges in the road network",
                metric
            ))
            .total(edge_rtree_entries.len())
            .build()?,
    ));

    // compute the modal metric for each edge in parallel via par_iter
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

    // write to file
    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);

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
