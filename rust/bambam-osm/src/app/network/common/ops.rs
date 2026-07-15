use super::MIN_DISTANCE_RTREE_NEIGHBOR;
use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use crate::model::osm::graph::OsmNodeDataSerializable;
use crate::model::osm::graph::OsmWayDataSerializable;
use rstar::RTree;
use std::error::Error;

/// Find the neighboring ways in the RTree from a given way centroid
pub fn find_neighboring_ways<'a>(
    query_entry: &WayRTreeEntry,
    rtree: &'a RTree<WayRTreeEntry>,
) -> Vec<&'a WayRTreeEntry> {
    rtree
        .locate_within_distance(
            [query_entry.centroid.x(), query_entry.centroid.y()],
            MIN_DISTANCE_RTREE_NEIGHBOR,
        )
        .filter(|entry_in_rtree| entry_in_rtree.way.osmid != query_entry.way.osmid)
        .collect()
}

/// Load ways from a CSV file and create R-tree entries for each way.
/// TODO: incorporate Overture's way attributes and move the logic for WayRTreeEntry up to bambam-core.
pub fn load_way_rtree_entries(
    edges_file: &str,
    nodes: &[OsmNodeDataSerializable],
) -> Result<Vec<WayRTreeEntry>, Box<dyn Error>> {
    let mut edge_reader = csv::Reader::from_path(edges_file)?;
    let mut way_entries = Vec::new();

    for record in edge_reader.deserialize::<OsmWayDataSerializable>() {
        let way = match record {
            Ok(way) => way,
            Err(err) => {
                eprintln!("Error reading row: {err}");
                continue;
            }
        };

        if nodes.get(way.src_vertex_id.0).is_none() {
            eprintln!(
                "Warning: source vertex {} not found for way {}; skipping",
                way.src_vertex_id.0, way.osmid
            );
            continue;
        }

        let osmid = way.osmid;
        let Some(entry) = WayRTreeEntry::new(way) else {
            eprintln!("Warning: could not create R-tree entry for way {osmid}");
            continue;
        };

        way_entries.push(entry);
    }

    Ok(way_entries)
}
