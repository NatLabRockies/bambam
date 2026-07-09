use crate::{
    app::network::{
        common::{way_ops::find_neighboring_ways, way_rtree_entry::WayRTreeEntry},
        wci::{
            cycle_score::compute_cycle_score, traffic_speed_score::compute_traffic_speed_score,
            wci_ops::*,
        },
    },
    model::osm::graph::OsmNodeDataSerializable,
};
use rstar::RTree;

/// Computes the WCI score for a given way (as WayRTreeEntry), the way's source node,
/// and the R-tree of all ways in the network.
pub fn compute_wci(
    rtree: &RTree<WayRTreeEntry>,
    entry: &WayRTreeEntry,
    src_node: &OsmNodeDataSerializable,
) -> Option<i32> {
    let way_is_walk_eligible = way_is_walk_eligible(rtree, entry);

    // Return the worst WCI score if the way is not eligible for walking
    // The worst WCI score is:
    // -2 (speed limit 50+ mph) +
    // -2 (no cycleway nor signage) +
    // -2 (no sidewalks) +
    // 0 (no traffic signals or stop signs)
    if !way_is_walk_eligible {
        return Some(-6);
    }

    let sidewalk_score = match &entry.way.sidewalk {
        Some(_) => 2,
        None => -2,
    };

    let neighboring_ways = find_neighboring_ways(rtree, &entry.centroid);

    let cycle_score = compute_cycle_score(entry, &neighboring_ways);

    let speed_score = compute_traffic_speed_score(entry, &neighboring_ways);

    let signal_or_stop_score: i32;

    if src_node.has_traffic_light() {
        signal_or_stop_score = 2;
    } else if src_node.has_stop_sign() {
        signal_or_stop_score = 1;
    } else {
        signal_or_stop_score = 0;
    }

    Some(cycle_score + speed_score + sidewalk_score + signal_or_stop_score)
}
