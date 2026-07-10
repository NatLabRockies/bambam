// Walk Comfort Index (WCI) Calculation in Rust
// Input: OSM data with attributes, Output: file with WCI scores for each way, one score per line
// Utilizes self-designed wayinfostruct and osminfostruct for data handling
// August 2025 EG

use super::way_attributes_for_wci::WayAttributesForWCI;
use super::way_geometry_and_data::WayGeometryData;
use crate::model::osm::graph::OsmNodeDataSerializable;
use crate::model::osm::graph::OsmWayDataSerializable;
use csv;
use geo::algorithm::centroid::Centroid;
use kdam::Bar;
use kdam::BarBuilder;
use kdam::BarExt;
use rayon::prelude::*;
use routee_compass_core::util::fs::read_utils;
use rstar::RTree;
use std::sync::Arc;
use std::sync::Mutex;
use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
};

// Process the WCI score by taking in a vertices-complete.csv and edges-complete.csv
// deserialize both of these, reading in information to construct an rtree
// process_wci stores the data in way_attributes_for_wci aand way_geometry_data structs
// wci_calculate calculates the WCI score for each way
// process_wci will print each score, line-by-line into an output .txt
pub fn process_wci(
    edges_file: &str,
    vertices_file: &str,
    output_file: &str,
) -> Result<(), Box<dyn Error>> {
    let nodes_bar = BarBuilder::default().desc("WCI: read vertices file");
    let nodes: Box<[OsmNodeDataSerializable]> =
        read_utils::from_csv(&vertices_file, true, Some(nodes_bar), None)?;
    let mut edges_reader = csv::Reader::from_path(edges_file)?;

    let mut centroids = vec![];
    let mut rtree_data = vec![];

    for row in edges_reader.deserialize() {
        match row {
            Ok(osm_data) => {
                let r: OsmWayDataSerializable = osm_data;
                let linestring = r.linestring.clone();
                let src_node = match nodes.get(r.src_vertex_id.0) {
                    Some(node) => node,
                    None => continue, // If source node is not found, skip this row
                };
                let has_stop = src_node
                    .clone()
                    .highway
                    .as_ref()
                    .is_some_and(|h| h.contains("stop"));
                let has_traf_sig = src_node
                    .clone()
                    .highway
                    .as_ref()
                    .is_some_and(|h| h.contains("traffic_signals"));
                if let Some(centroid) = linestring.centroid() {
                    let centroid_geo: geo::Point<f32> = geo::Point::new(centroid.x(), centroid.y());
                    centroids.push(centroid_geo);
                    let rtree_entry = WayGeometryData {
                        geo: linestring,
                        data: r,
                        stop: has_stop,
                        traf_sig: has_traf_sig,
                    };
                    rtree_data.push(rtree_entry);
                }
            }
            Err(err) => {
                eprint!("Error reading row: {err}");
            }
        }
    }

    let rtree = RTree::bulk_load(rtree_data.clone());

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .desc("WCI")
            .total(centroids.len())
            .build()?,
    ));
    let wci_vec: Vec<(i32, i32, i32, i32, i32)> = centroids
        .into_par_iter()
        .enumerate()
        .map(
            |(idx, centroid)| match WayAttributesForWCI::new(centroid, &rtree, &rtree_data[idx]) {
                Some(w) => w.wci_components(),
                None => (-6, 0, 0, 0, 0),
            },
        )
        .collect();
    println!("wci_vec is {wci_vec:?}");

    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);

    writeln!(writer, "wci_total,wci_walk,wci_speed,wci_cycle,wci_signal")?;

    for (total, walk, speed, cycle, signal) in wci_vec {
        writeln!(writer, "{},{},{},{},{}", total, walk, speed, cycle, signal)?;
    }

    Ok(())
}
