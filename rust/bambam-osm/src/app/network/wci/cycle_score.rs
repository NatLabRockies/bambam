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
    // Guards against division by zero for coincident centroids.
    const EPSILON: f32 = 1e-6;
    const NO_CYCLEWAY_FOUND_SCORE: i32 = -2;
    // (score, weight) for each neighbor that actually has a cycleway tag
    let scored: Vec<(i32, f32)> = neighboring_ways
        .iter()
        .filter_map(|neighbor| {
            // grab the neighbor's cycleway attribute
            let tag = neighbor.way.cycleway.as_ref()?;

            // create the score from this neighbor's cycleway spec
            let score = cycle_score_from_tag(&CyclewayTag::new(tag));

            // compute distance from the centroid to the neighbor.
            let distance = Euclidean.distance(entry.centroid, neighbor.centroid);

            // inverse of distance weights closer weighs higher.
            let weight = 1.0 / (distance + EPSILON);
            Some((score, weight))
        })
        .collect();

    if scored.is_empty() {
        return NO_CYCLEWAY_FOUND_SCORE;
    }

    let total_weight: f32 = scored.iter().map(|&(_, w)| w).sum();
    let weighted_score: f32 = scored.iter().map(|&(s, w)| s as f32 * w).sum::<f32>() / total_weight;

    weighted_score.round() as i32
}
