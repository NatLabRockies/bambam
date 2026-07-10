use crate::app::network::wci::compute_wci::compute_wci;
use crate::app::network::{
    common::load_way_rtree_entry::load_way_rtree_entries, wci::compute_wci::WCIComponentScores,
};
use crate::model::osm::graph::OsmNodeDataSerializable;
use rayon::prelude::*;
use routee_compass_core::util::fs::read_utils;
use rstar::RTree;
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
    // load all vertices into memory
    let nodes: Box<[OsmNodeDataSerializable]> =
        read_utils::from_csv(&vertices_file, true, None, None)?;

    // load all ways into memory as type WayRTreeEntry for insertion into the R-tree
    let way_rtree_entries = load_way_rtree_entries(edges_file, &nodes)?;

    // bulk-load a copy of the entries into the r-tree; we keep `entries` around
    // so each centroid can be paired with its own way during the WCI calculation
    let rtree = RTree::bulk_load(way_rtree_entries.clone());

    // calculate the WCI component scores for each way in parallel using the compute_wci function
    let wci_vec_with_components: Vec<WCIComponentScores> = way_rtree_entries
        .par_iter()
        .map(|way_entry| {
            compute_wci(
                &rtree,
                way_entry,
                nodes
                    .get(way_entry.way.src_vertex_id.0)
                    .unwrap_or(&OsmNodeDataSerializable::default()),
            )
        })
        .collect();

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
            wci.walk_score.map_or(String::new(), |v| v.to_string()),
            wci.traffic_speed_score
                .map_or(String::new(), |v| v.to_string()),
            wci.cycle_score.map_or(String::new(), |v| v.to_string()),
            wci.traffic_signal_score
                .map_or(String::new(), |v| v.to_string()),
        )?;
    }
    writer.flush()?;

    Ok(())
}
