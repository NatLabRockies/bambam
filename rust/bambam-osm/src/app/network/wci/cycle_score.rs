use crate::app::network::common::cycleway_tag::CyclewayTag::{
    self, DedicatedNoBuffer, NoDedicatedNoFacilities, NoDedicatedWithFacilities,
};
use crate::app::network::common::way_rtree_entry::WayRTreeEntry;
use geo::{Distance, Euclidean};

/// Computes the cycleway score for a way.
/// If the way has a cycleway tag, uses the tag to determine the score.
/// Otherwise, computes a weighted score based on neighboring ways' cycleway tags.
pub fn compute_cycle_score(entry: &WayRTreeEntry, neighboring_ways: &Vec<&WayRTreeEntry>) -> i32 {
    match &entry.way.cycleway {
        Some(tag) => cycle_score_from_tag(&CyclewayTag::new(tag)),
        None => cycle_score_from_neighbors(entry, neighboring_ways),
    }
}

/// Converts a cycleway tag classification to a numerical score.
pub fn cycle_score_from_tag(tag: &CyclewayTag) -> i32 {
    match tag {
        DedicatedNoBuffer => 2,
        NoDedicatedWithFacilities => 0,
        NoDedicatedNoFacilities => -2,
    }
}

/// Computes an inverse-distance-weighted cycleway score from neighboring
/// ways, so neighbors closer to the center way have more influence.
pub fn cycle_score_from_neighbors(
    entry: &WayRTreeEntry,
    neighboring_ways: &[&WayRTreeEntry],
) -> i32 {
    const NO_CYCLEWAY_FOUND_SCORE: i32 = -2;

    // NOTE (old-pipeline parity): the denominator sums distances over ALL
    // neighbors, tagged or not, so weights don't sum to 1 and the result is
    // attenuated toward 0. Farther neighbors also weigh MORE (direct, not
    // inverse, distance weighting). Both quirks are inherited deliberately.
    let mut total_distance: f32 = 0.0;
    let mut scored: Vec<(i32, f32)> = Vec::new();

    for neighbor in neighboring_ways {
        let distance = Euclidean.distance(entry.centroid, neighbor.centroid);
        total_distance += distance;
        if let Some(tag) = neighbor.way.cycleway.as_ref() {
            scored.push((cycle_score_from_tag(&CyclewayTag::new(tag)), distance));
        }
    }

    if scored.is_empty() || total_distance == 0.0 {
        return NO_CYCLEWAY_FOUND_SCORE;
    }

    let weighted: f32 = scored
        .iter()
        .map(|&(score, d)| score as f32 * (d / total_distance))
        .sum();

    weighted as i32 // truncation, matching the old `result_cycle as i32`
}
