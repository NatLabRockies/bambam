use geo::{Distance, Euclidean};

use crate::app::network::common::way_rtree_entry::WayRTreeEntry;

pub fn compute_traffic_speed_score(entry: &WayRTreeEntry, neighbors: &Vec<&WayRTreeEntry>) -> i32 {
    traffic_speed_score_from_maxspeed(entry)
        .or_else(|| Some(traffic_speed_score_from_neighbors(entry, neighbors)))
        .unwrap_or(2)
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

/// Computes a traffic speed score for a way based on its maxspeed tag.
/// Uses the existing `get_speed()` method which handles unit conversion.
pub fn traffic_speed_score_from_maxspeed(entry: &WayRTreeEntry) -> Option<i32> {
    const KMH_TO_MPH: f64 = 0.621371;
    match entry.way.get_speed("maxspeed", true) {
        Ok(Some(velocity)) => {
            let speed_kmh = velocity.get::<uom::si::velocity::kilometer_per_hour>();
            Some(score_from_speed((speed_kmh * KMH_TO_MPH) as i32))
        }
        _ => None,
    }
}

pub fn traffic_speed_score_from_neighbors(
    entry: &WayRTreeEntry,
    neighboring_ways: &Vec<&WayRTreeEntry>,
) -> i32 {
    // const NO_MAXSPEED_FOUND_SCORE: i32 = -2;

    /* if neighboring_ways.is_empty() {
        return NO_MAXSPEED_FOUND_SCORE;
    } */

    // Collect scores and inverse-distance weights for all neighbors that have a maxspeed
    let scored: Vec<(i32, f32)> = neighboring_ways
        .iter()
        .filter_map(|neighbor| {
            traffic_speed_score_from_maxspeed(neighbor).map(|score| {
                let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
                (score, distance)
            })
        })
        .collect();

    // If no neighbors have valid maxspeed values, return default score
    /* if scored.is_empty() {
        return NO_MAXSPEED_FOUND_SCORE;
    } */

    // Compute weighted average: sum of (score * weight) / sum of weights
    let sum_distances: f32 = scored.iter().map(|(_, dist)| dist).sum();

    /* if sum_distances == 0.0 {
        return NO_MAXSPEED_FOUND_SCORE;
    } */

    let weighted_score: f32 = scored
        .iter()
        .map(|(score, distance)| (*score as f32) * (distance / sum_distances)) // weight is distance/sum_distances
        .sum::<f32>();

    weighted_score.round() as i32
}
