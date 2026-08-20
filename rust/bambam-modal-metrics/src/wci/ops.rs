use geo::{Distance, Euclidean};

use crate::common::cycleway_tag::CyclewayTag;
use crate::common::edge_rtree_entry::{EdgeRTreeEntry, MIN_DISTANCE_RTREE_NEIGHBOR};
use crate::common::ops::estimated_speed_from_neighbors;
use crate::network_traits::{edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge};
use crate::wci::NO_CYCLEWAY_FOUND_SCORE;
use rstar::RTree;

/// Converts a cycleway tag classification to a numerical comfort index.
pub fn cycleway_comfort_from_tag(tag: &CyclewayTag) -> i32 {
    match tag {
        CyclewayTag::DedicatedWithBuffer => 2,
        CyclewayTag::DedicatedNoBuffer => 2,
        CyclewayTag::NoDedicatedWithFacilities => 0,
        CyclewayTag::NoDedicatedNoFacilities => -2,
    }
}

/// Computes the cycleway comfort index from neighboring edges
pub fn cycleway_comfort_from_neighbors<E: SpatialEdge + EdgeForModalMetric>(
    entry: &EdgeRTreeEntry<E>,
    neighboring_edges: &[&EdgeRTreeEntry<E>],
) -> i32 {
    let mut total_distance: f32 = 0.0;
    let mut scored: Vec<(i32, f32)> = Vec::new();

    for neighbor in neighboring_edges {
        let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
        total_distance += distance;
        if let Some(tag) = neighbor.edge.get_cycleway_tag() {
            scored.push((cycleway_comfort_from_tag(&tag), distance));
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

/// Computes a weighted traffic speed comfort index from nearby edges if the
/// edge of interest does not have a speed limit.
pub fn traffic_speed_comfort_from_neighbors<E: SpatialEdge + EdgeForModalMetric>(
    entry: &EdgeRTreeEntry<E>,
    neighboring_edges: &[&EdgeRTreeEntry<E>],
) -> i32 {
    let speed_mph = estimated_speed_from_neighbors(entry, neighboring_edges).unwrap_or(0.0);
    traffic_speed_comfort_from_speed(speed_mph.round() as i32)
}

/// Determines if the edge is walk-eligible based on its own attributes or nearby sidewalk edges.
pub fn is_walk_eligible<E: SpatialEdge + EdgeForModalMetric>(
    rtree: &RTree<EdgeRTreeEntry<E>>,
    entry: &EdgeRTreeEntry<E>,
) -> bool {
    entry.edge.is_walkable()
}
