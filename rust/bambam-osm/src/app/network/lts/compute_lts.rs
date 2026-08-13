use rstar::RTree;

use crate::app::network::{
    common::{
        cycleway_tag::CyclewayTag,
        ops::{estimated_speed_from_neighbors, find_neighboring_ways, traffic_speed_from_maxspeed},
        way_rtree_entry::WayRTreeEntry,
    },
    lts::{
        lts::{Lts, LtsError, MAX_LTS, MIN_LTS},
        ops::{is_non_motorized_way, is_unbikeable_way},
    },
};

/// Computes the level of traffic stress for a given way entry.
pub fn compute_lts(rtree: &RTree<WayRTreeEntry>, entry: &WayRTreeEntry) -> Result<Lts, LtsError> {
    // Some ways are inherently unsuitable for bikes.
    if is_unbikeable_way(&entry.way.highway) {
        return Lts::new(MAX_LTS);
    }
    // A highway that is non-motorized is inherently low-stress
    if is_non_motorized_way(&entry.way.highway) {
        return Lts::new(MIN_LTS);
    }

    let speed = traffic_speed_from_maxspeed(entry).unwrap_or_else(|| {
        let neighboring_ways = find_neighboring_ways(entry, rtree);
        estimated_speed_from_neighbors(entry, &neighboring_ways).unwrap_or(25.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::network::common::cycleway_tag::CyclewayTag;

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
