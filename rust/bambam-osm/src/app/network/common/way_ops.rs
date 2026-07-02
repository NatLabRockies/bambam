use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use geo::Point;
use rstar::RTree;

/// Find the neighboring ways in the RTree from a given way centroid
pub fn find_neighboring_ways<'a>(
    rtree: &'a RTree<WayRTreeEntry>,
    way_centroid: &Point<f32>,
) -> Vec<&'a WayRTreeEntry> {
    rtree
        .locate_within_distance([way_centroid.x(), way_centroid.y()], 0.0001378)
        .collect()
}
