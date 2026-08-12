use crate::app::network::common::modal_metric::{ModalMetric, ModalMetricError, ModalMetricValue};
use crate::app::network::common::ops::load_way_rtree_entries;
use crate::model::osm::graph::OsmNodeDataSerializable;
use kdam::{Bar, BarBuilder, BarExt};
use rayon::prelude::*;
use routee_compass_core::util::fs::read_utils;
use rstar::RTree;
use std::sync::{Arc, Mutex};
use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
};

/// Bulk compute a specific modal metric for all ways in an OSM network by taking in a vertices-complete.csv
/// and edges-complete.csv.
///
/// `metric_name` can either be:
/// - "WCI" for the Walking Comfort Index metric
/// - "LTS" for the Level of Traffic Stress (cycling comfort) metric
pub fn bulk_compute_modal_metric(
    metric_name: &str,
    edges_file: &str,
    vertices_file: &str,
    output_file: &str,
) -> Result<(), Box<dyn Error>> {
    let metric: ModalMetric = metric_name.parse()?;

    log::info!(
        "\nLoading files for {:?} modal metric computation.\n",
        metric
    );
    log::info!("Reading:\n\t- vertex set @ {vertices_file}\n\t- edge set @ {edges_file}\n");

    let vertices: Box<[OsmNodeDataSerializable]> =
        read_utils::from_csv(&vertices_file, true, None, None)?;
    let way_rtree_entries = load_way_rtree_entries(edges_file, &vertices)?;
    log::info!("Edges and vertices read successfully.\n");

    let rtree = RTree::bulk_load(way_rtree_entries.clone());

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .desc(format!(
                "Computing {:?} for ways in the road network",
                metric
            ))
            .total(way_rtree_entries.len())
            .build()?,
    ));

    let values: Vec<ModalMetricValue> = way_rtree_entries
        .par_iter()
        .map(|way_entry| {
            let src_node = vertices.get(way_entry.way.src_vertex_id.0);

            let result = metric.compute_metric(&rtree, way_entry, src_node)?;

            if let Ok(mut bar) = bar.lock() {
                let _ = bar.update(1);
            }

            Ok(result)
        })
        .collect::<Result<Vec<ModalMetricValue>, ModalMetricError>>()?;

    eprintln!();

    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);

    metric.write_csv_header(&mut writer)?;
    for value in &values {
        value.write_csv_row(&mut writer)?;
    }
    writer.flush()?;

    log::info!(
        "\n\n{:?} values computed successfully.\n\nOutputfile saved @ {output_file}.",
        metric
    );
    Ok(())
}
