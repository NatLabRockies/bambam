use crate::app::network::common::modal_penalty::{
    ModalPenalty, ModalPenaltyError, ModalPenaltyResult,
};
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

/// Bulk compute modal penalty scores for an OSM network by taking in a vertices-complete.csv
/// and edges-complete.csv.
///
/// penalty_name can either be:
/// - "WCI" for Walking Comfort Index
/// - "LTS" for Level of Traffic Stress (cycling comfort)
pub fn bulk_compute_modal_penalty(
    penalty_name: &str,
    edges_file: &str,
    vertices_file: &str,
    output_file: &str,
) -> Result<(), Box<dyn Error>> {
    let penalty: ModalPenalty = penalty_name.parse()?;

    println!("Loading files for {:?} modal penalty computation.", penalty);
    println!("Reading:\n\t- vertex set @ {vertices_file}\n\t- edge set @ {edges_file}");

    let vertices: Box<[OsmNodeDataSerializable]> =
        read_utils::from_csv(&vertices_file, true, None, None)?;
    let way_rtree_entries = load_way_rtree_entries(edges_file, &vertices)?;
    println!("Edges and vertices read successfully.");

    let rtree = RTree::bulk_load(way_rtree_entries.clone());

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .desc(format!(
                "Computing {:?} modal penalty scores for the road network",
                penalty
            ))
            .total(way_rtree_entries.len())
            .build()?,
    ));

    let default_node = OsmNodeDataSerializable::default();
    let score_results: Vec<ModalPenaltyResult> = way_rtree_entries
        .par_iter()
        .map(|way_entry| {
            let src_node = vertices
                .get(way_entry.way.src_vertex_id.0)
                .unwrap_or(&default_node);

            let result = penalty.compute_score(&rtree, way_entry, src_node)?;

            if let Ok(mut bar) = bar.lock() {
                let _ = bar.update(1);
            }

            Ok(result)
        })
        .collect::<Result<Vec<ModalPenaltyResult>, ModalPenaltyError>>()?;

    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);

    penalty.write_csv_header(&mut writer)?;
    for score in &score_results {
        score.write_csv_row(&mut writer)?;
    }
    writer.flush()?;

    println!(
        "{:?} scores computed successfully.\nScores file saved @ {output_file}.",
        penalty
    );
    Ok(())
}
