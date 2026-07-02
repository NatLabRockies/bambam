use geo::{Distance, Euclidean};

use crate::app::network::common::way_rtree_entry::WayRTreeEntry;

pub fn compute_traffic_speed_score(entry: &WayRTreeEntry, neighbors: &Vec<&WayRTreeEntry>) -> i32 {
    traffic_speed_score_from_maxspeed(entry)
        .or_else(|| Some(traffic_speed_score_from_neighbors(entry, neighbors)))
        .unwrap_or(-2)
}

/// Converts a speed in KMH to MPH and then to a numerical score.
fn score_from_speed(speed_kmh: i32) -> i32 {
    const KMH_TO_MPH: f32 = 0.621371;
    let speed_mph = (speed_kmh as f32 * KMH_TO_MPH).round() as i32;

    if speed_mph > 0 && speed_mph <= 25 {
        2
    } else if speed_mph > 25 && speed_mph <= 30 {
        1
    } else if speed_mph > 30 && speed_mph <= 40 {
        0
    } else if speed_mph > 40 && speed_mph <= 45 {
        -1
    } else {
        -2 // NOTE: This case includes when we don't find a max speed (speed = 0)
    }
}

/// Computes a traffic speed score for a way based on its maxspeed tag.
/// Uses the existing `get_speed()` method which handles delimited values and unit conversion.
pub fn traffic_speed_score_from_maxspeed(entry: &WayRTreeEntry) -> Option<i32> {
    match entry.way.get_speed("maxspeed", true) {
        Ok(Some(velocity)) => {
            let speed_kmh = velocity.get::<uom::si::velocity::kilometer_per_hour>() as i32;
            Some(score_from_speed(speed_kmh))
        }
        _ => None,
    }
}

pub fn traffic_speed_score_from_neighbors(
    entry: &WayRTreeEntry,
    neighboring_ways: &Vec<&WayRTreeEntry>,
) -> i32 {
    const EPSILON: f32 = 1e-6; // Minimum distance to account for divbyzero
    const NO_MAXSPEED_FOUND_SCORE: i32 = -2;

    if neighboring_ways.is_empty() {
        return NO_MAXSPEED_FOUND_SCORE;
    }

    // Collect scores and inverse-distance weights for all neighbors that have a maxspeed
    let scored: Vec<(i32, f32)> = neighboring_ways
        .iter()
        .filter_map(|neighbor| {
            traffic_speed_score_from_maxspeed(neighbor).map(|score| {
                let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
                let weight = 1.0 / (distance + EPSILON);
                (score, weight)
            })
        })
        .collect();

    // If no neighbors have valid maxspeed values, return default score
    if scored.is_empty() {
        return NO_MAXSPEED_FOUND_SCORE;
    }

    // Compute weighted average: sum of (score * weight) / sum of weights
    let total_weight: f32 = scored.iter().map(|(_, w)| w).sum();

    if total_weight == 0.0 {
        return NO_MAXSPEED_FOUND_SCORE;
    }

    let weighted_score: f32 = scored
        .iter()
        .map(|(score, weight)| (*score as f32) * weight)
        .sum::<f32>()
        / total_weight;

    weighted_score.round() as i32
}
