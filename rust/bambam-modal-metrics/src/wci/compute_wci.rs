use rstar::RTree;

use crate::common::edge_rtree_entry::{find_neighboring_edges, EdgeRTreeEntry};
use crate::network_traits::{
    edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge,
    vertex_for_modal_metric::VertexForModalMetric,
};
use crate::wci::ops::is_walk_eligible;
use crate::wci::wci::{Wci, WciError, MAX_WCI, MIN_WCI};

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
    /// Returns the minimum WCI (no components)
    pub fn min_wci() -> Result<Self, WciError> {
        Ok(Self {
            total: Wci::new(MIN_WCI)?,
            ..Default::default()
        })
    }
    /// Returns the maximum WCI (no components)
    pub fn max_wci() -> Result<Self, WciError> {
        Ok(Self {
            total: Wci::new(MAX_WCI)?,
            ..Default::default()
        })
    }
}

/// Computes the walking comfort index (WCI) score for a given edge (as EdgeRTreeEntry),
/// the edge's source vertex, and the R-tree of all edges in the network.
pub fn compute_wci<E, V>(
    rtree: &RTree<EdgeRTreeEntry<E>>,
    entry: &EdgeRTreeEntry<E>,
    src_node: Option<&V>,
) -> Result<WciComponents, WciError>
where
    E: SpatialEdge + EdgeForModalMetric,
    V: VertexForModalMetric,
{
    // general walk-eligibility based on edge attributes and neighbors.
    let is_walk_eligible = is_walk_eligible(rtree, entry);

    // grab the neighboring edges
    let neighboring_edges = find_neighboring_edges(entry, rtree);

    if !is_walk_eligible {
        // Total WCI score = Min WCI score (unwalkable roadway)
        WciComponents::min_wci()
    } else if entry.edge.is_footway() || (neighboring_edges.is_empty() && entry.edge.is_sidewalk())
    {
        // Total WCI score = Max WCI score (footway or sidewalk with no adjacent ways)
        WciComponents::max_wci()
    } else {
        // Compute all component scores.

        let walkability = Wci::walkability(&entry.edge);

        let cycleway_comfort = Wci::cycleway_comfort(entry, &neighboring_edges);

        let traffic_speed_comfort = Wci::traffic_speed_comfort(entry, &neighboring_edges);

        let traffic_signal_comfort = src_node
            .map(|v| Wci::traffic_signal_comfort(v))
            .unwrap_or_else(|| Wci::ZERO);

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
    use super::*;
    use crate::common::cycleway_tag::CyclewayTag;
    use geo::LineString;

    #[derive(Clone)]
    struct TestEdge {
        id: usize,
        walkable: bool,
        sidewalk: bool,
        footway: bool,
        speed_limit: Option<i32>,
        cycleway: Option<CyclewayTag>,
        linestring: LineString<f32>,
    }

    impl SpatialEdge for TestEdge {
        fn id(&self) -> String {
            self.id.to_string()
        }
        fn src_vertex_id(&self) -> usize {
            0
        }
        fn linestring(&self) -> std::option::Option<&geo::LineString<f32>> {
            Some(&self.linestring)
        }
    }

    impl EdgeForModalMetric for TestEdge {
        fn get_traffic_speed_limit(&self) -> Option<i32> {
            self.speed_limit
        }
        fn get_cycleway_tag(&self) -> Option<CyclewayTag> {
            self.cycleway.clone()
        }
        fn is_walkable(&self) -> bool {
            self.walkable
        }
        fn is_walkable_highway(&self) -> bool {
            false
        }
        fn is_sidewalk(&self) -> bool {
            self.sidewalk
        }
        fn is_footway(&self) -> bool {
            self.footway
        }
        fn is_unbikeable(&self) -> bool {
            false
        }
        fn is_non_motorized(&self) -> bool {
            false
        }
        fn is_oneway(&self) -> bool {
            false
        }
    }

    struct TestVertex {
        has_signals: bool,
        has_stop: bool,
    }

    impl VertexForModalMetric for TestVertex {
        fn has_traffic_signals(&self) -> bool {
            self.has_signals
        }
        fn has_stop_sign(&self) -> bool {
            self.has_stop
        }
    }

    #[test]
    fn test_min_wci() {
        let edge = TestEdge {
            id: 42,
            walkable: false,
            sidewalk: false,
            footway: false,
            speed_limit: Some(65),
            cycleway: None,
            linestring: LineString::from(vec![(-105.170016, 39.773648), (-105.165381, 39.774176)]),
        };
        let src_vertex = TestVertex {
            has_signals: false,
            has_stop: false,
        };

        let entry = EdgeRTreeEntry::new(edge).unwrap();
        let rtree: RTree<EdgeRTreeEntry<TestEdge>> = RTree::new();

        let wci = compute_wci(&rtree, &entry, Some(&src_vertex)).unwrap();
        assert_eq!(wci.total, Wci::new(MIN_WCI).unwrap());
    }

    #[test]
    fn test_max_wci() {
        let edge = TestEdge {
            id: 42,
            walkable: true,
            sidewalk: false,
            footway: true,
            speed_limit: None,
            cycleway: None,
            linestring: LineString::from(vec![(-105.170016, 39.773648), (-105.165381, 39.774176)]),
        };
        let src_vertex = TestVertex {
            has_signals: false,
            has_stop: false,
        };

        let entry = EdgeRTreeEntry::new(edge).unwrap();
        let rtree: RTree<EdgeRTreeEntry<TestEdge>> = RTree::new();

        let wci = compute_wci(&rtree, &entry, Some(&src_vertex)).unwrap();
        assert_eq!(wci.total, Wci::new(MAX_WCI).unwrap());
    }

    #[test]
    fn test_positive_wci() {
        let edge = TestEdge {
            id: 42,
            walkable: true,
            sidewalk: false,
            footway: false,
            speed_limit: Some(25),
            cycleway: Some(CyclewayTag::NoDedicatedWithFacilities),
            linestring: LineString::from(vec![(-105.170016, 39.773648), (-105.165381, 39.774176)]),
        };
        let src_vertex = TestVertex {
            has_signals: false,
            has_stop: true,
        };

        let entry = EdgeRTreeEntry::new(edge).unwrap();
        let rtree: RTree<EdgeRTreeEntry<TestEdge>> = RTree::new();

        let wci = compute_wci(&rtree, &entry, Some(&src_vertex)).unwrap();
        assert_eq!(wci.traffic_speed_comfort, Some(Wci::new(2).unwrap()));
        assert_eq!(wci.traffic_signal_comfort, Some(Wci::new(1).unwrap()));
        assert_eq!(wci.cycleway_comfort, Some(Wci::new(0).unwrap()));
        assert_eq!(wci.walkability, Some(Wci::new(-2).unwrap()));
        assert!(wci.total > Wci::new(0).unwrap());
    }

    #[test]
    fn test_negative_wci() {
        let edge = TestEdge {
            id: 42,
            walkable: true,
            sidewalk: false,
            footway: false,
            speed_limit: Some(45),
            cycleway: None,
            linestring: LineString::from(vec![(-105.170016, 39.773648), (-105.165381, 39.774176)]),
        };
        let src_vertex = TestVertex {
            has_signals: false,
            has_stop: true,
        };

        let entry = EdgeRTreeEntry::new(edge).unwrap();
        let rtree: RTree<EdgeRTreeEntry<TestEdge>> = RTree::new();

        let wci = compute_wci(&rtree, &entry, Some(&src_vertex)).unwrap();
        assert_eq!(wci.traffic_speed_comfort, Some(Wci::new(-1).unwrap()));
        assert_eq!(wci.traffic_signal_comfort, Some(Wci::new(1).unwrap()));
        assert_eq!(wci.cycleway_comfort, Some(Wci::new(-2).unwrap()));
        assert_eq!(wci.walkability, Some(Wci::new(-2).unwrap()));
        assert_eq!(wci.total, Wci::new(-4).unwrap());
        assert!(wci.total < Wci::new(0).unwrap());
    }

    #[test]
    fn test_neighbor_wci_contribution() {
        const WAY_SCORE_NO_NEIGHBORS: i32 = -4;
        let edge = TestEdge {
            id: 42,
            walkable: true,
            sidewalk: false,
            footway: false,
            speed_limit: Some(45),
            cycleway: None,
            linestring: LineString::from(vec![(-105.170016, 39.773648), (-105.165381, 39.774176)]),
        };

        let neighbor = TestEdge {
            id: 43,
            walkable: true,
            sidewalk: false,
            footway: false,
            speed_limit: Some(25),
            cycleway: Some(CyclewayTag::DedicatedNoBuffer),
            linestring: LineString::from(vec![(-105.168085, 39.773772), (-105.166755, 39.773937)]),
        };

        let src_vertex = TestVertex {
            has_signals: false,
            has_stop: true,
        };

        let entry = EdgeRTreeEntry::new(edge).unwrap();
        let neighbor_entry = EdgeRTreeEntry::new(neighbor).unwrap();
        let mut rtree: RTree<EdgeRTreeEntry<TestEdge>> = RTree::new();

        rtree.insert(entry.clone());
        rtree.insert(neighbor_entry);
        let wci = compute_wci(&rtree, &entry, Some(&src_vertex)).unwrap();
        assert!(wci.total > Wci::new(WAY_SCORE_NO_NEIGHBORS).unwrap());
    }
}
