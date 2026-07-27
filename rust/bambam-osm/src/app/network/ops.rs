use crate::model::osm::graph::OsmNodeDataSerializable;
use crate::model::osm::graph::OsmWayDataSerializable;
use bambam_core::network::rtree_entry::EdgeRTreeEntry;
use std::error::Error;

/// Load ways from a CSV file and create generic R-tree entries for each way.
pub fn load_edge_rtree_entries(
    edges_file: &str,
    nodes: &[OsmNodeDataSerializable],
) -> Result<Vec<EdgeRTreeEntry<OsmWayDataSerializable>>, Box<dyn Error>> {
    let mut edge_reader = csv::Reader::from_path(edges_file)?;
    let mut entries = Vec::new();

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
        let Some(entry) = EdgeRTreeEntry::new(way) else {
            eprintln!("Warning: could not create R-tree entry for way {osmid}");
            continue;
        };

        entries.push(entry);
    }

    Ok(entries)
}
