use super::super::MIN_DISTANCE_RTREE_NEIGHBOR;
use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use crate::model::feature::highway::Highway;
use crate::model::osm::graph::OsmWayDataSerializable;
use rstar::RTree;

/// Computes the walk score, which is determined by
/// if it has a sidewalk or a footway.
/// Args:
/// - way: the OsmWayDataSerializable to compute the score for
pub fn compute_walk_score(way: &OsmWayDataSerializable) -> i32 {
    if way_is_sidewalk(way) || way_is_footway(way) {
        2
    } else {
        -2
    }
}
/// Determines if a way is walk-eligible based on sidewalk/footway attributes or highway type.
///
/// Args:
/// - way: the OsmWayDataSerializable to check
fn is_walkable(way: &OsmWayDataSerializable) -> bool {
    let is_sidewalk = way_is_sidewalk(way);

    let is_footway = way_is_footway(way);

    let is_walkable_highway = way_is_walkable_highway(way);

    is_sidewalk || is_footway || is_walkable_highway
}

/// Determines if the way is walk-eligible based on it's OSM attributes.
/// If the way is not walk-eligible, checks if any neighboring ways within a distance of 15 meters are walk-eligible.
///
/// Args:
/// - `rtree`: RTree of all ways in the network
/// - `entry`: The way of interest (as WayRTreeEntry)
pub fn way_is_walk_eligible(rtree: &RTree<WayRTreeEntry>, entry: &WayRTreeEntry) -> bool {
    is_walkable(&entry.way) // check the way itself
        || rtree // check neighboring ways
            .locate_within_distance([entry.centroid.x(), entry.centroid.y()], MIN_DISTANCE_RTREE_NEIGHBOR)
            .any(|neighbor| way_is_sidewalk(&neighbor.way))
}

// Checks if the way is a sidewalk
pub fn way_is_sidewalk(way: &OsmWayDataSerializable) -> bool {
    way.sidewalk
        .as_ref()
        .is_some_and(|s| s != "no" && s != "none")
        || way.footway == Some("sidewalk".to_string())
}

/// Checks if the way is a footway
pub fn way_is_footway(way: &OsmWayDataSerializable) -> bool {
    way.footway
        .as_ref()
        .is_some_and(|s| s != "no" && s != "none")
}

/// A walkable highway is a normal roadway that is typically low traffic or
/// low speed.
pub fn way_is_walkable_highway(way: &OsmWayDataSerializable) -> bool {
    matches!(
        way.highway,
        Highway::Residential
            | Highway::Unclassified
            | Highway::LivingStreet
            | Highway::Service
            | Highway::Pedestrian
            | Highway::Trailhead
            | Highway::Track
            | Highway::Footway
            | Highway::Bridleway
            | Highway::Steps
            | Highway::Corridor
            | Highway::Path
            | Highway::Elevator
    )
}
