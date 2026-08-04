use super::NO_CYCLEWAY_FOUND_SCORE;
use crate::app::network::common::cycleway_tag::CyclewayTag::{
    self, DedicatedNoBuffer, DedicatedWithBuffer, NoDedicatedNoFacilities,
    NoDedicatedWithFacilities,
};
use crate::app::network::common::ops::estimated_speed_from_neighbors;
use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use crate::app::network::common::MIN_DISTANCE_RTREE_NEIGHBOR;
use crate::model::feature::highway::Highway;
use crate::model::osm::graph::{OsmNodeDataSerializable, OsmWayDataSerializable};
use geo::{Distance, Euclidean};
use rstar::RTree;

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

/// returns true if the node has a stop sign
pub fn has_stop_sign(node: &OsmNodeDataSerializable) -> bool {
    node.clone()
        .highway
        .as_ref()
        .is_some_and(|highway| highway.contains("stop"))
}

/// returns true if the node has a traffic light
pub fn has_traffic_signals(node: &OsmNodeDataSerializable) -> bool {
    node.clone()
        .highway
        .as_ref()
        .is_some_and(|highway| highway.contains("traffic_signals"))
}

/// Converts a cycleway tag classification to a numerical comfort index.
pub fn cycleway_comfort_from_tag(tag: &CyclewayTag) -> i32 {
    match tag {
        DedicatedWithBuffer => 2,
        DedicatedNoBuffer => 2,
        NoDedicatedWithFacilities => 0,
        NoDedicatedNoFacilities => -2,
    }
}

/// Computes the cycleway comfort index from neighboring ways
pub fn cycleway_comfort_from_neighbors(
    entry: &WayRTreeEntry,
    neighboring_ways: &[&WayRTreeEntry],
) -> i32 {
    let mut total_distance: f32 = 0.0;
    let mut scored: Vec<(i32, f32)> = Vec::new();

    for neighbor in neighboring_ways {
        let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
        total_distance += distance;
        if let Some(tag) = neighbor.way.cycleway.as_ref() {
            scored.push((cycleway_comfort_from_tag(&CyclewayTag::new(tag)), distance));
        }
    }

    if scored.is_empty() || total_distance == 0.0 {
        return NO_CYCLEWAY_FOUND_SCORE;
    }

    let weighted: f32 = scored
        .iter()
        .map(|&(score, d)| score as f32 * (d / total_distance))
        .sum();

    weighted as i32
}

/// Converts a speed in MPH to a numerical comfort index.
pub fn traffic_speed_comfort_from_speed(speed_mph: i32) -> i32 {
    if speed_mph <= 25 {
        2
    } else if speed_mph > 25 && speed_mph <= 30 {
        1
    } else if speed_mph > 30 && speed_mph <= 40 {
        0
    } else if speed_mph > 40 && speed_mph <= 45 {
        -1
    } else {
        -2
    }
}

/// Computes a weighted traffic speed comfort index from nearby ways if the
/// way of interest does not have a speed limit
pub fn traffic_speed_comfort_from_neighbors(
    entry: &WayRTreeEntry,
    neighboring_ways: &[&WayRTreeEntry],
) -> i32 {
    let speed_mph = estimated_speed_from_neighbors(entry, neighboring_ways).unwrap_or(0.0);
    traffic_speed_comfort_from_speed(speed_mph.round() as i32)
}
