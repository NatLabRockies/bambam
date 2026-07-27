use super::wci_score::WciScore;
use crate::network::cycleway_tag::CyclewayTag;
use crate::network::penalty_traits::{EdgeForModalPenalties, SpatialEdge, VertexForModalPenalties};
use crate::network::rtree_entry::EdgeRTreeEntry;
use geo::{Distance, Euclidean};

/// If there is no cycleway found for an edge or its neighbors, use this as the
/// cycleway component of the WCI.
pub const NO_CYCLEWAY_FOUND_SCORE: i32 = -2;

/// Determines whether an edge is walk-eligible: either it is walkable on its
/// own attributes, or one of its spatial neighbors is a sidewalk.
pub fn is_walk_eligible<E>(entry: &EdgeRTreeEntry<E>, neighbors: &[&EdgeRTreeEntry<E>]) -> bool
where
    E: EdgeForModalPenalties + SpatialEdge,
{
    entry.edge.is_walkable() || neighbors.iter().any(|n| n.edge.is_sidewalk())
}

/// Converts a cycleway tag classification to a numerical score.
pub fn cycleway_score_from_tag(tag: &CyclewayTag) -> i32 {
    match tag {
        CyclewayTag::DedicatedNoBuffer => 2,
        CyclewayTag::NoDedicatedWithFacilities => 0,
        CyclewayTag::NoDedicatedNoFacilities => -2,
    }
}

/// Computes the cycleway score from neighboring edges, distance-weighted by the
/// centroid distance to each neighbor that carries a cycleway tag.
pub fn cycleway_score_from_neighbors<E>(
    entry: &EdgeRTreeEntry<E>,
    neighbors: &[&EdgeRTreeEntry<E>],
) -> i32
where
    E: EdgeForModalPenalties + SpatialEdge,
{
    let mut total_distance: f32 = 0.0;
    let mut scored: Vec<(i32, f32)> = Vec::new();

    for neighbor in neighbors {
        let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
        total_distance += distance;
        if let Some(tag) = neighbor.edge.get_cycleway_tag() {
            scored.push((cycleway_score_from_tag(&tag), distance));
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

/// Converts a speed in MPH to a numerical score.
pub fn traffic_speed_score_from_speed(speed_mph: i32) -> i32 {
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

/// Computes a distance-weighted traffic speed score from nearby edges when the
/// edge of interest does not carry a speed limit.
pub fn traffic_speed_score_from_neighbors<E>(
    entry: &EdgeRTreeEntry<E>,
    neighbors: &[&EdgeRTreeEntry<E>],
) -> i32
where
    E: EdgeForModalPenalties + SpatialEdge,
{
    let speeds_and_distances: Vec<(f32, f32)> = neighbors
        .iter()
        .filter_map(|neighbor| {
            neighbor.edge.get_traffic_speed_limit().map(|speed| {
                let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
                (speed as f32, distance)
            })
        })
        .collect();

    let sum_distances: f32 = speeds_and_distances
        .iter()
        .map(|(_, distance)| *distance)
        .sum();

    if sum_distances == 0.0 {
        // no neighbors with a known speed limit; treat as the worst case.
        return traffic_speed_score_from_speed(i32::MAX);
    }

    let weighted_speed: f32 = speeds_and_distances
        .iter()
        .map(|(speed, distance)| speed * distance / sum_distances)
        .sum();

    traffic_speed_score_from_speed(weighted_speed.round() as i32)
}

/// Computes the walkability `WciScore` for an edge.
pub fn walkability_score<E: EdgeForModalPenalties>(edge: &E) -> WciScore {
    if edge.is_sidewalk() || edge.is_footway() {
        WciScore::from_component(2)
    } else {
        WciScore::from_component(-2)
    }
}

/// Computes the traffic signal `WciScore` for an edge's source vertex.
pub fn traffic_signal_score<V: VertexForModalPenalties>(src_vertex: &V) -> WciScore {
    if src_vertex.has_traffic_signals() {
        WciScore::from_component(2)
    } else if src_vertex.has_stop_sign() {
        WciScore::from_component(1)
    } else {
        WciScore::from_component(0)
    }
}

/// Computes the cycleway `WciScore` for an edge, using the edge's own cycleway
/// tag if present, otherwise inferring from neighbors.
pub fn cycleway_score<E>(entry: &EdgeRTreeEntry<E>, neighbors: &[&EdgeRTreeEntry<E>]) -> WciScore
where
    E: EdgeForModalPenalties + SpatialEdge,
{
    match entry.edge.get_cycleway_tag() {
        Some(tag) => WciScore::from_component(cycleway_score_from_tag(&tag)),
        None => WciScore::from_component(cycleway_score_from_neighbors(entry, neighbors)),
    }
}

/// Computes the traffic speed `WciScore` for an edge, using the edge's own
/// speed limit if present, otherwise inferring from neighbors.
pub fn traffic_speed_score<E>(
    entry: &EdgeRTreeEntry<E>,
    neighbors: &[&EdgeRTreeEntry<E>],
) -> WciScore
where
    E: EdgeForModalPenalties + SpatialEdge,
{
    WciScore::from_component(
        entry
            .edge
            .get_traffic_speed_limit()
            .map(traffic_speed_score_from_speed)
            .unwrap_or_else(|| traffic_speed_score_from_neighbors(entry, neighbors)),
    )
}
