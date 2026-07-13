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

#[cfg(test)]
mod test {
    use super::compute_wci;
    use crate::{
        app::network::{
            common::way_rtree_entry::WayRTreeEntry,
            wci::{compute_wci::WCIComponentScores, MAX_WCI_SCORE, MIN_WCI_SCORE},
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

        let score: WCIComponentScores = compute_wci(&rtree, &entry, &src_vertex);
        assert_eq!(score.total_score, MIN_WCI_SCORE);
    }

    /// An isolated footway gives the max WCI score.
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

        let score: WCIComponentScores = compute_wci(&rtree, &entry, &src_vertex);
        assert_eq!(score.total_score, MAX_WCI_SCORE);
    }
    #[test]
    fn test_positive_wci() {
        // a residential roadway with speed limit 25mph, a shared-lane
        // cycleway, and a stop sign at the source node
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
        let score: WCIComponentScores = compute_wci(&rtree, &entry, &src_vertex);
        assert_eq!(score.traffic_speed_score, Some(2));
        assert_eq!(score.traffic_signal_score, Some(1));
        assert_eq!(score.cycle_score, Some(0));
        assert_eq!(score.walk_score, Some(-2));
        assert!(score.total_score > 0)
    }

    #[test]
    fn test_negative_wci() {
        // a residential highway with speed limit 45 mph and a stop sign at the source node
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

        // compute wci for the residential highway with nearby sidewalk
        let score: WCIComponentScores = compute_wci(&rtree, &entry, &src_vertex);
        assert_eq!(score.traffic_speed_score, Some(-1));
        assert_eq!(score.traffic_signal_score, Some(1));
        assert_eq!(score.cycle_score, Some(-2));
        assert_eq!(score.walk_score, Some(-2));
        assert!(score.total_score < 0)
    }
}
