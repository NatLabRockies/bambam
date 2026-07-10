use geo::{Distance, Euclidean};

use crate::app::network::common::way_rtree_entry::WayRTreeEntry;

/// Computes the traffic speed score for a way.
///
/// If the way does not have a speed limit sign, we perform a nearest neighbor search
/// with the RTree and use neighboring ways to compute a weighted score.
pub fn compute_traffic_speed_score(entry: &WayRTreeEntry, neighbors: &Vec<&WayRTreeEntry>) -> i32 {
    traffic_speed_from_maxspeed(entry)
        .map(|speed_mph| score_from_speed(speed_mph.round() as i32))
        .unwrap_or_else(|| traffic_speed_score_from_neighbors(entry, neighbors))
}

/// Converts a speed in MPH to a numerical score.
fn score_from_speed(speed_mph: i32) -> i32 {
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

/// Converts from whichever OSM maxspeed unit to MPH
pub fn traffic_speed_from_maxspeed(entry: &WayRTreeEntry) -> Option<f32> {
    const KMH_TO_MPH: f64 = 0.621371;
    match entry.way.get_speed("maxspeed", true) {
        Ok(Some(velocity)) => {
            let speed_kmh = velocity.get::<uom::si::velocity::kilometer_per_hour>();
            Some((speed_kmh * KMH_TO_MPH) as f32) // speed in mph
        }
        _ => None,
    }
}

/// Computes a weighted traffic speed score from nearby ways if the
/// way of interest does not have a speed limit
pub fn traffic_speed_score_from_neighbors(
    entry: &WayRTreeEntry,
    neighboring_ways: &Vec<&WayRTreeEntry>,
) -> i32 {
    let speeds_and_distances: Vec<(f32, f32)> = neighboring_ways
        .iter()
        .filter_map(|neighbor| {
            traffic_speed_from_maxspeed(neighbor).map(|speed| {
                let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
                (speed, distance)
            })
        })
        .collect();

    let sum_distances: f32 = speeds_and_distances
        .iter()
        .map(|(_, distance)| *distance)
        .sum();

    let weighted_speed: f32 = speeds_and_distances
        .iter()
        .map(|(speed, distance)| speed * distance / sum_distances)
        .sum();

    score_from_speed(weighted_speed.round() as i32)
}
