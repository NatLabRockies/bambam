use super::super::MIN_DISTANCE_RTREE_NEIGHBOR;
use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use rstar::RTree;

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
