use rstar::RTree;

use crate::app::network::{
    common::{
        cycleway_tag::CyclewayTag,
        ops::{estimated_speed_from_neighbors, find_neighboring_ways, traffic_speed_from_maxspeed},
        way_rtree_entry::WayRTreeEntry,
    },
    lts::lts::{Lts, LtsError},
};

/// Computes the level of traffic stress for a given way entry.
pub fn compute_lts(rtree: &RTree<WayRTreeEntry>, entry: &WayRTreeEntry) -> Result<Lts, LtsError> {
    let speed = traffic_speed_from_maxspeed(entry).unwrap_or_else(|| {
        let neighboring_ways = find_neighboring_ways(entry, rtree);
        estimated_speed_from_neighbors(entry, &neighboring_ways).unwrap_or(0.0)
    });

    let cycleway_tag = entry
        .way
        .cycleway
        .as_ref()
        .map(|tag| CyclewayTag::new(tag))
        .unwrap_or(CyclewayTag::NoDedicatedNoFacilities);

    let oneway = entry.way.oneway.as_deref() == Some("yes");

    Lts::from_table_lookup(speed.round() as u8, cycleway_tag, oneway)
}
