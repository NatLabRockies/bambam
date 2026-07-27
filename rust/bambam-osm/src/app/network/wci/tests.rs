use crate::model::osm::graph::{OsmNodeDataSerializable, OsmWayDataSerializable};
use bambam_core::network::rtree_entry::EdgeRTreeEntry;
use bambam_core::network::wci::{
    compute_wci, WciComponentScores, WciScore, MAX_WCI_SCORE, MIN_WCI_SCORE,
};
use rstar::RTree;

type OsmEntry = EdgeRTreeEntry<OsmWayDataSerializable>;

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

    let entry = EdgeRTreeEntry::new(way).unwrap();
    let rtree: RTree<OsmEntry> = RTree::new(); // just need this to pass into wci, not using it.

    let score: WciComponentScores = compute_wci(&rtree, &entry, &src_vertex).unwrap();
    assert_eq!(score.total_score, WciScore::new(MIN_WCI_SCORE).unwrap());
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

    let entry = EdgeRTreeEntry::new(way).unwrap();
    let rtree: RTree<OsmEntry> = RTree::new(); // just need this to pass into wci, not using it.

    let score: WciComponentScores = compute_wci(&rtree, &entry, &src_vertex).unwrap();
    assert_eq!(score.total_score, WciScore::new(MAX_WCI_SCORE).unwrap());
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

    let entry = EdgeRTreeEntry::new(way).unwrap();
    let rtree: RTree<OsmEntry> = RTree::new(); // just need this to pass into wci, not using it.

    // compute wci for the residential highway with nearby sidewalk
    let score: WciComponentScores = compute_wci(&rtree, &entry, &src_vertex).unwrap();
    assert_eq!(score.traffic_speed_score, Some(WciScore::new(2).unwrap()));
    assert_eq!(score.traffic_signal_score, Some(WciScore::new(1).unwrap()));
    assert_eq!(score.cycleway_score, Some(WciScore::new(0).unwrap()));
    assert_eq!(score.walkability_score, Some(WciScore::new(-2).unwrap()));
    assert!(score.total_score > WciScore::new(0).unwrap());
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

    let entry = EdgeRTreeEntry::new(way).unwrap();
    let rtree: RTree<OsmEntry> = RTree::new(); // just need this to pass into wci, not using it.

    // compute wci
    let score: WciComponentScores = compute_wci(&rtree, &entry, &src_vertex).unwrap();
    assert_eq!(score.traffic_speed_score, Some(WciScore::new(-1).unwrap()));
    assert_eq!(score.traffic_signal_score, Some(WciScore::new(1).unwrap()));
    assert_eq!(score.cycleway_score, Some(WciScore::new(-2).unwrap()));
    assert_eq!(score.walkability_score, Some(WciScore::new(-2).unwrap()));
    assert_eq!(score.total_score, WciScore::new(-4).unwrap());
    assert!(score.total_score < WciScore::new(0).unwrap());
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

    let entry = EdgeRTreeEntry::new(way).unwrap();
    let neighbor_entry = EdgeRTreeEntry::new(neighbor).unwrap();
    let mut rtree: RTree<OsmEntry> = RTree::new();

    rtree.insert(entry.clone());
    rtree.insert(neighbor_entry);
    let score = compute_wci(&rtree, &entry, &src_vertex).unwrap();
    assert!(score.total_score > WciScore::new(WAY_SCORE_NO_NEIGHBORS).unwrap());
}
