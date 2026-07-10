use super::{MAX_WCI_SCORE, MIN_WCI_SCORE};
use crate::{
    app::network::{
        common::{way_ops::find_neighboring_ways, way_rtree_entry::WayRTreeEntry},
        wci::{
            cycle_score::compute_cycle_score,
            traffic_signal_score::compute_traffic_signal_score,
            traffic_speed_score::compute_traffic_speed_score,
            walk_score::{
                compute_walk_score, way_has_footway, way_has_sidewalk, way_is_walk_eligible,
            },
        },
    },
    model::osm::graph::OsmNodeDataSerializable,
};
use rstar::RTree;

/// These are the WCI scores, including total
/// and all components that went into the score.
///
/// Note:
///
/// If all of the component scores are zero but the total score
/// is not, this means the way was either chosen to have the max
/// WCI score (super walkable), or the min WCI score (unwalkable)
pub struct WCIComponentScores {
    pub total_score: i32,
    pub walk_score: Option<i32>,
    pub traffic_speed_score: Option<i32>,
    pub cycle_score: Option<i32>,
    pub traffic_signal_score: Option<i32>,
}

/// Computes the walking comfort index (WCI) score for a given way (as WayRTreeEntry),
/// the way's source node, and the R-tree of all ways in the network.
pub fn compute_wci(
    rtree: &RTree<WayRTreeEntry>,
    entry: &WayRTreeEntry,
    src_node: &OsmNodeDataSerializable,
) -> WCIComponentScores {
    let way_is_walk_eligible = way_is_walk_eligible(rtree, entry);

    let neighboring_ways = find_neighboring_ways(entry, rtree);

    if !way_is_walk_eligible {
        // Min WCI score (the way is not eligible for walking)
        WCIComponentScores {
            total_score: MIN_WCI_SCORE,
            walk_score: None,
            traffic_speed_score: None,
            cycle_score: None,
            traffic_signal_score: None,
        }
    } else if way_has_footway(&entry.way)
        || (neighboring_ways.is_empty() && way_has_sidewalk(&entry.way))
    {
        // Max WCI score (walking path away from roads)
        WCIComponentScores {
            total_score: MAX_WCI_SCORE,
            walk_score: None,
            traffic_speed_score: None,
            cycle_score: None,
            traffic_signal_score: None,
        }
    } else {
        let walk_score = compute_walk_score(&entry.way);

        let cycle_score = compute_cycle_score(entry, &neighboring_ways);

        let traffic_speed_score = compute_traffic_speed_score(entry, &neighboring_ways);

        let traffic_signal_score = compute_traffic_signal_score(src_node);

        // component score
        WCIComponentScores {
            total_score: walk_score + traffic_speed_score + cycle_score + traffic_signal_score,
            walk_score: Some(walk_score),
            traffic_speed_score: Some(traffic_speed_score),
            cycle_score: Some(cycle_score),
            traffic_signal_score: Some(traffic_signal_score),
        }
    }
}
