use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use crate::model::feature::highway::Highway;
use crate::model::osm::graph::OsmWayDataSerializable;
use rstar::RTree;

const MIN_DISTANCE_NEIGHBOR: f32 = 0.0001378; // minimum distance to consider a way as a neighbor [deg] ~ 15m

/// Determines if a way is walk-eligible based on sidewalk/footway attributes or highway type.
///
/// Args:
/// - way: the OsmWayDataSerializable to check
fn is_walkable(way: &OsmWayDataSerializable) -> bool {
    let has_valid_sidewalk = way
        .sidewalk
        .as_ref()
        .is_some_and(|s| s != "no" && s != "none")
        || way.footway == Some("sidewalk".to_string());

    let has_valid_footway = way
        .footway
        .as_ref()
        .is_some_and(|s| s != "no" && s != "none");

    let is_walkable_highway = matches!(
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
    );

    has_valid_sidewalk || has_valid_footway || is_walkable_highway
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
            .locate_within_distance([entry.centroid.x(), entry.centroid.y()], MIN_DISTANCE_NEIGHBOR)
            .any(|neighbor| is_walkable(&neighbor.way))
}
