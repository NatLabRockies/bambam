use crate::{
    app::network::{
        common::{ops::find_neighboring_ways, way_rtree_entry::WayRTreeEntry},
        wci::{
            ops::*,
            wci::{Wci, WciError, MAX_WCI, MIN_WCI},
        },
    },
    model::osm::graph::OsmNodeDataSerializable,
};
use rstar::RTree;

/// The Walking Comfort Index (WCI) components for a way, including total WCI
/// and all components that went into the total WCI.
#[derive(Default)]
pub struct WciComponents {
    pub total: Wci,
    pub walkability: Option<Wci>,
    pub traffic_speed_comfort: Option<Wci>,
    pub cycleway_comfort: Option<Wci>,
    pub traffic_signal_comfort: Option<Wci>,
}

impl WciComponents {
    pub fn min_wci_score() -> Result<Self, WciError> {
        Ok(Self {
            total: Wci::new(MIN_WCI)?,
            ..Default::default()
        })
    }

    pub fn max_wci_score() -> Result<Self, WciError> {
        Ok(Self {
            total: Wci::new(MAX_WCI)?,
            ..Default::default()
        })
    }
}

/// Computes the walking comfort index (WCI) score for a given way (as WayRTreeEntry),
/// the way's source node, and the R-tree of all ways in the network.
pub fn compute_wci(
    rtree: &RTree<WayRTreeEntry>,
    entry: &WayRTreeEntry,
    src_node: &OsmNodeDataSerializable,
) -> Result<WciComponents, WciError> {
    let way_is_walk_eligible = way_is_walk_eligible(rtree, entry);

    let neighboring_ways = find_neighboring_ways(entry, rtree);

    if !way_is_walk_eligible {
        // Total WCI score = Min WCI score (unwalkable roadway)
        WciComponents::min_wci_score()
    } else if way_is_footway(&entry.way)
        || (neighboring_ways.is_empty() && way_is_sidewalk(&entry.way))
    {
        // Total WCI score = Max WCI score (footway or sidewalk with no adjacent ways)
        WciComponents::max_wci_score()
    } else {
        let walkability = Wci::walkability(&entry.way);

        let cycleway_comfort = Wci::cycleway_comfort(entry, &neighboring_ways);

        let traffic_speed_comfort = Wci::traffic_speed_comfort(entry, &neighboring_ways);

        let traffic_signal_comfort = Wci::traffic_signal_comfort(src_node);

        // Total = Sum of WCI component scores
        Ok(WciComponents {
            total: &walkability
                + &traffic_speed_comfort
                + &cycleway_comfort
                + &traffic_signal_comfort,
            walkability: Some(walkability),
            traffic_speed_comfort: Some(traffic_speed_comfort),
            cycleway_comfort: Some(cycleway_comfort),
            traffic_signal_comfort: Some(traffic_signal_comfort),
        })
    }
}

#[cfg(test)]
mod test {
    use super::compute_wci;
    use super::WciComponents;
    use crate::{
        app::network::{
            common::way_rtree_entry::WayRTreeEntry,
            wci::{wci::MAX_WCI, wci::MIN_WCI, Wci},
        },
        model::osm::graph::{OsmNodeDataSerializable, OsmWayDataSerializable},
    };
    use rstar::RTree;
    use serde_json;

    /// Unwalkable highway gives the minimum WCI score
    #[test]
    fn test_min_wci() {
        let way: OsmWayDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 42,
            "src_vertex_id": 0,
            "dst_vertex_id": 1,
            "highway": "motorway",
            "maxspeed": "65 mph",
            "linestring": "LINESTRING (-105.170016 39.773648, -105.165381 39.774176)",
            "length_meters": 400.0
        }"#,
        )
        .unwrap();

        let src_vertex: OsmNodeDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 0,
            "x": -105.170016,
            "y": 39.773648
        }"#,
        )
        .unwrap();

        let entry = WayRTreeEntry::new(way).unwrap();
        let rtree: RTree<WayRTreeEntry> = RTree::new(); // just need this to pass into wci, not using it.

        let wci: WciComponents = compute_wci(&rtree, &entry, &src_vertex).unwrap();
        assert_eq!(wci.total, Wci::new(MIN_WCI).unwrap());
    }

    /// A footway gives the max WCI score.
    #[test]
    fn test_max_wci() {
        let way: OsmWayDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 42,
            "src_vertex_id": 0,
            "dst_vertex_id": 1,
            "highway": "footway",
            "footway": "alley",
            "maxspeed": "",
            "linestring": "LINESTRING (-105.170016 39.773648, -105.165381 39.774176)",
            "length_meters": 400.0
        }"#,
        )
        .unwrap();

        let src_vertex: OsmNodeDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 0,
            "x": -105.170016,
            "y": 39.773648
        }"#,
        )
        .unwrap();

        let entry = WayRTreeEntry::new(way).unwrap();
        let rtree: RTree<WayRTreeEntry> = RTree::new(); // just need this to pass into wci, not using it.

        let wci: WciComponents = compute_wci(&rtree, &entry, &src_vertex).unwrap();
        assert_eq!(wci.total, Wci::new(MAX_WCI).unwrap());
    }

    // a residential roadway with speed limit 25mph, a shared-lane
    // cycleway, and a stop sign at the source node should have a positive wci score
    #[test]
    fn test_positive_wci() {
        let way: OsmWayDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 42,
            "src_vertex_id": 0,
            "dst_vertex_id": 1,
            "highway": "residential",
            "cycleway": "shared_lane",
            "maxspeed": "25 mph",
            "linestring": "LINESTRING (-105.170016 39.773648, -105.165381 39.774176)",
            "length_meters": 400.0
        }"#,
        )
        .unwrap();

        let src_vertex: OsmNodeDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 0,
            "x": -105.170016,
            "y": 39.773648,
            "highway": "stop"
        }"#,
        )
        .unwrap();

        let entry = WayRTreeEntry::new(way).unwrap();
        let rtree: RTree<WayRTreeEntry> = RTree::new(); // just need this to pass into wci, not using it.

        // compute wci for the residential highway with nearby sidewalk
        let wci: WciComponents = compute_wci(&rtree, &entry, &src_vertex).unwrap();
        assert_eq!(wci.traffic_speed_comfort, Some(Wci::new(2).unwrap()));
        assert_eq!(wci.traffic_signal_comfort, Some(Wci::new(1).unwrap()));
        assert_eq!(wci.cycleway_comfort, Some(Wci::new(0).unwrap()));
        assert_eq!(wci.walkability, Some(Wci::new(-2).unwrap()));
        assert!(wci.total > Wci::new(0).unwrap());
    }

    // A residential highway with speed limit 45 mph and a stop sign at the source node
    // should have a negative WCI score
    #[test]
    fn test_negative_wci() {
        let way: OsmWayDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 42,
            "src_vertex_id": 0,
            "dst_vertex_id": 1,
            "highway": "residential",
            "maxspeed": "45 mph",
            "linestring": "LINESTRING (-105.170016 39.773648, -105.165381 39.774176)",
            "length_meters": 400.0
        }"#,
        )
        .unwrap();

        let src_vertex: OsmNodeDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 0,
            "x": -105.170016,
            "y": 39.773648,
            "highway": "stop"
        }"#,
        )
        .unwrap();

        let entry = WayRTreeEntry::new(way).unwrap();
        let rtree: RTree<WayRTreeEntry> = RTree::new(); // just need this to pass into wci, not using it.

        // compute wci
        let wci: WciComponents = compute_wci(&rtree, &entry, &src_vertex).unwrap();
        assert_eq!(wci.traffic_speed_comfort, Some(Wci::new(-1).unwrap()));
        assert_eq!(wci.traffic_signal_comfort, Some(Wci::new(1).unwrap()));
        assert_eq!(wci.cycleway_comfort, Some(Wci::new(-2).unwrap()));
        assert_eq!(wci.walkability, Some(Wci::new(-2).unwrap()));
        assert_eq!(wci.total, Wci::new(-4).unwrap());
        assert!(wci.total < Wci::new(0).unwrap());
    }

    /// A residential highway with a bad score get's its
    /// score buffed by a neighboring road with cycleway and low speed limit
    #[test]
    fn test_neighbor_wci_contribution() {
        const WAY_SCORE_NO_NEIGHBORS: i32 = -4; // from the previous test
        let way: OsmWayDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 42,
            "src_vertex_id": 0,
            "dst_vertex_id": 1,
            "highway": "residential",
            "maxspeed": "45 mph",
            "linestring": "LINESTRING (-105.170016 39.773648, -105.165381 39.774176)",
            "length_meters": 400.0
        }"#,
        )
        .unwrap();

        // This neighbor has a cycleway, and a low speed limit, so it's
        // weighted score should contribute positively to the query's score
        let neighbor: OsmWayDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 43,
            "src_vertex_id": 2,
            "dst_vertex_id": 3,
            "highway": "residential",
            "maxspeed": "25 mph",
            "cycleway": "lane",
            "linestring": "LINESTRING (-105.168085 39.773772, -105.166755 39.773937)",
            "length_meters": 100
        }"#,
        )
        .unwrap();

        let src_vertex: OsmNodeDataSerializable = serde_json::from_str(
            r#"{
            "osmid": 0,
            "x": -105.170016,
            "y": 39.773648,
            "highway": "stop"
        }"#,
        )
        .unwrap();

        let entry = WayRTreeEntry::new(way).unwrap();
        let neighbor_entry = WayRTreeEntry::new(neighbor).unwrap();
        let mut rtree: RTree<WayRTreeEntry> = RTree::new();

        rtree.insert(entry.clone());
        rtree.insert(neighbor_entry);
        let wci = compute_wci(&rtree, &entry, &src_vertex).unwrap();
        assert!(wci.total > Wci::new(WAY_SCORE_NO_NEIGHBORS).unwrap());
    }
}
