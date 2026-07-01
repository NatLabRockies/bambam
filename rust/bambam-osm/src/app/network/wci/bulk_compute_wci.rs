// Walk Comfort Index (WCI) Calculation in Rust
// Input: OSM data with attributes, Output: file with WCI scores for each way, one score per line
// August 2025 EG

use super::way_attributes_for_wci::WayAttributesForWCI;
use crate::app::network::common::load_way_rtree_entry::load_way_rtree_entries;
use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
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
pub fn bulk_compute_wci(
    edges_file: &str,
    vertices_file: &str,
    output_file: &str,
) -> Result<(), Box<dyn Error>> {
    // load all vertices into memory
    let nodes: Box<[OsmNodeDataSerializable]> =
        read_utils::from_csv(&vertices_file, true, None, None)?;

    // load all ways into memory as type WayRTreeEntry and create R-tree entries for each way
    let way_rtree_entries: Vec<WayRTreeEntry> = load_way_rtree_entries(edges_file, &nodes)?;

    // bulk-load a copy of the entries into the r-tree; we keep `entries` around
    // so each centroid can be paired with its own way during the WCI calculation
    let rtree = RTree::bulk_load(way_rtree_entries.clone());

    // calculate the WCI score for each (centroid, way) pair in parallel
    // I don't think we need to use centroids, since WayRTreeEntry already has the linestring.
    // We can just use way.linestring.centroid() in WayAttributesForWCI::new() instead of passing in the centroid.
    let wci_vec: Vec<i32> = way_rtree_entries
        .par_iter() // parallel iterator over way entries
        .filter_map(|entry| {
            WayAttributesForWCI::new(
                &rtree,
                &entry.way,
                nodes
                    .get(entry.way.src_vertex_id.0)
                    .unwrap_or(&OsmNodeDataSerializable::default()),
            ) // need to change the signature to &rtree, &OsmWayDataSerializable
            .and_then(|w: WayAttributesForWCI| w.wci_calculate()) // calculate the WCI score for this way
        }) // filter out any None values (failed calculations)
        .collect(); // collect the wci scores into a vector

    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);
    for wci in wci_vec {
        writeln!(writer, "{wci}")?;
    }
    writer.flush()?;

    Ok(())
}
