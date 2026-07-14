use crate::app::network::wci::compute_wci::compute_wci;
use crate::app::network::wci::wci_score::WciError;
use crate::app::network::{
    common::ops::load_way_rtree_entries, wci::compute_wci::WciComponentScores,
};
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

/// Bulk compute WCI scores for an OSM network by taking in a vertices-complete.csv
/// and edges-complete.csv.
///
/// Reads the verticies and edges into memory, constructs an R-tree for spatial queries,
/// computes the WCI score for each way in parallel, and writes the scores to an output file.
///
/// WCI scores are computed in parallel and written line-by-line to `output_file`.
/// Authors: EG (2025 original), AM (2026 refactor)
pub fn bulk_compute_wci(
    edges_file: &str,
    vertices_file: &str,
    output_file: &str,
) -> Result<(), Box<dyn Error>> {
    println!("Reading:\n\t- vertex set @ {vertices_file}\n\t- edge set @ {edges_file}");
    // load all vertices into memory
    let vertices: Box<[OsmNodeDataSerializable]> =
        read_utils::from_csv(&vertices_file, true, None, None)?;
    // load all ways into memory as type WayRTreeEntry for insertion into the R-tree
    let way_rtree_entries = load_way_rtree_entries(edges_file, &vertices)?;
    println!("Edges and vertices read successfully.");
    // bulk-load a copy of the entries into the r-tree; we keep `entries` around
    // so each centroid can be paired with its own way during the WCI calculation
    let rtree = RTree::bulk_load(way_rtree_entries.clone());

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .desc(format!("Computing WCI scores for the road network"))
            .total(way_rtree_entries.len())
            .build()?,
    ));

    // calculate the WCI component scores for each way in parallel using the compute_wci function
    let wci_vec_with_components: Vec<WciComponentScores> = way_rtree_entries
        .par_iter()
        .map(|way_entry| -> Result<WciComponentScores, WciError> {
            // compute wci for this entry
            let wci = compute_wci(
                &rtree,
                way_entry,
                vertices
                    .get(way_entry.way.src_vertex_id.0)
                    .unwrap_or(&OsmNodeDataSerializable::default()),
            );
            // update pbar
            if let Ok(mut bar) = bar.clone().lock() {
                let _ = bar.update(1);
            }
            // return wci result
            wci
        })
        .collect::<Result<Vec<WciComponentScores>, WciError>>()?;

    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);
    // write header
    writeln!(writer, "wci_total,wci_walk,wci_speed,wci_cycle,wci_signal")?;
    // write values
    for wci in &wci_vec_with_components {
        writeln!(
            writer,
            "{},{},{},{},{}",
            wci.total_score,
            wci.walkability_score
                .as_ref()
                .map_or(String::new(), |v| v.to_string()),
            wci.traffic_speed_score
                .as_ref()
                .map_or(String::new(), |v| v.to_string()),
            wci.cycleway_score
                .as_ref()
                .map_or(String::new(), |v| v.to_string()),
            wci.traffic_signal_score
                .as_ref()
                .map_or(String::new(), |v| v.to_string()),
        )?;
    }
    writer.flush()?;
    println!("WCI scores computed successfully.\nWCI scores file saved @ {output_file}.");
    Ok(())
}
