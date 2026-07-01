use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use crate::model::osm::graph::OsmNodeDataSerializable;
use crate::model::osm::graph::OsmWayDataSerializable;
use std::error::Error;

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
