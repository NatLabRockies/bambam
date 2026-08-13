use rstar::RTree;

use crate::common::cycleway_tag::CyclewayTag;
use crate::common::edge_rtree_entry::{EdgeRTreeEntry, find_neighboring_edges};
use crate::common::ops::{estimated_speed_from_neighbors, traffic_speed_from_maxspeed};
use crate::lts::lts::{Lts, LtsError, MAX_LTS, MIN_LTS};
use crate::network_traits::{edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge};

/// Computes the level of traffic stress for a given edge entry.
pub fn compute_lts<E>(
    rtree: &RTree<EdgeRTreeEntry<E>>,
    entry: &EdgeRTreeEntry<E>,
) -> Result<Lts, LtsError>
where
    E: SpatialEdge + EdgeForModalMetric,
{
    // Some ways are inherently unsuitable for bikes.
    if entry.edge.is_unbikeable() {
        return Lts::new(MAX_LTS);
    }
    // A highway that is non-motorized is inherently low-stress
    if entry.edge.is_non_motorized() {
        return Lts::new(MIN_LTS);
    }

    let speed = traffic_speed_from_maxspeed(entry).unwrap_or_else(|| {
        let neighboring_ways = find_neighboring_edges(entry, rtree);
        estimated_speed_from_neighbors(entry, &neighboring_ways).unwrap_or(25.0)
    });

    let cycleway_tag = entry
        .edge
        .get_cycleway_tag()
        .unwrap_or(CyclewayTag::NoDedicatedNoFacilities);

    let oneway = entry.edge.is_oneway();

    Lts::from_table_lookup(speed.round() as u8, cycleway_tag, oneway)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::cycleway_tag::CyclewayTag;

    #[test]
    fn dedicated_with_buffer_is_always_lts1() {
        let lts = Lts::from_table_lookup(70, CyclewayTag::DedicatedWithBuffer, false).unwrap();
        assert_eq!(lts, Lts::new(1).unwrap());
    }

    #[test]
    fn no_dedicated_no_facilities_high_speed_is_lts4() {
        let lts = Lts::from_table_lookup(50, CyclewayTag::NoDedicatedNoFacilities, false).unwrap();
        assert_eq!(lts, Lts::new(4).unwrap());
    }

    #[test]
    fn no_dedicated_no_facilities_low_speed_is_lts2() {
        let lts = Lts::from_table_lookup(20, CyclewayTag::NoDedicatedNoFacilities, false).unwrap();
        assert_eq!(lts, Lts::new(2).unwrap());
    }

    #[test]
    fn dedicated_no_buffer_high_speed_is_lts4() {
        let lts = Lts::from_table_lookup(45, CyclewayTag::DedicatedNoBuffer, false).unwrap();
        assert_eq!(lts, Lts::new(4).unwrap());
    }

    #[test]
    fn no_dedicated_with_facilities_high_speed_is_lts4() {
        let lts =
            Lts::from_table_lookup(41, CyclewayTag::NoDedicatedWithFacilities, false).unwrap();
        assert_eq!(lts, Lts::new(4).unwrap());
    }
}
