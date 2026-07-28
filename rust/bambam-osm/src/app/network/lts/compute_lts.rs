use rstar::RTree;

use crate::{
    app::network::{
        common::way_rtree_entry::WayRTreeEntry,
        lts::lts_score::{LtsError, LtsScore},
    },
    model::osm::graph::OsmNodeDataSerializable,
};

pub fn compute_lts(
    rtree: &RTree<WayRTreeEntry>,
    entry: &WayRTreeEntry,
    src_node: &OsmNodeDataSerializable,
) -> Result<LtsScore, LtsError> {
    todo!();
}
