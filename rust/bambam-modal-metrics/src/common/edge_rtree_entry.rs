use std::marker::PhantomData;

use geo::{BoundingRect, Centroid, Distance, Euclidean};
use rstar::{PointDistance, RTreeObject, AABB};

use crate::{
    common::cycleway_tag::CyclewayTag,
    network_traits::{edge_for_modal_metric::EdgeForModalMetric, spatial_edge::SpatialEdge},
};

/// The maximum distance (in degrees) within which another edge is considered a
/// spatial "neighbor" for the purpose of neighbor-aware modal penalty scores.
/// Roughly 15 meters at mid latitudes.
pub const DISTANCE_RTREE_NEIGHBOR: f32 = 1.816e-8;

/// Compact, heap-free 16-byte representation of an edge's modal metric attributes
/// and identity.
///
/// Packing the [`EdgeForModalMetric`] predicates and [`SpatialEdge`] identifiers
/// into a `Copy` struct avoids retaining full `EdgeCompleteRecord` / `OsmWayDataSerializable`
/// structs (including heap `String`s and `LineString<f32>` coordinate vectors) inside
/// the spatial R-tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactEdgeAttrs {
    /// 64-bit hash of `SpatialEdge::id()`, stored as `[u32; 2]` to keep 4-byte alignment.
    id_hash: [u32; 2],
    /// Source vertex index (`SpatialEdge::src_vertex_id()`).
    src_vertex_id: u32,
    /// Posted traffic speed limit in MPH, or `i16::MIN` when unknown (`None`).
    speed_limit_mph: i16,
    /// Optional cycleway classification.
    cycleway_tag: Option<CyclewayTag>,
    /// Packed boolean predicates from [`EdgeForModalMetric`].
    flags: u8,
}

impl CompactEdgeAttrs {
    const FLAG_WALKABLE: u8 = 1 << 0;
    const FLAG_WALKABLE_HIGHWAY: u8 = 1 << 1;
    const FLAG_SIDEWALK: u8 = 1 << 2;
    const FLAG_FOOTWAY: u8 = 1 << 3;
    const FLAG_PEDESTRIAN_PRIORITY: u8 = 1 << 4;
    const FLAG_UNBIKEABLE: u8 = 1 << 5;
    const FLAG_NON_MOTORIZED: u8 = 1 << 6;
    const FLAG_ONEWAY: u8 = 1 << 7;

    /// Extracts all modal metric attributes from `edge` into a compact 16-byte value.
    #[inline]
    pub fn from_edge<E: SpatialEdge + EdgeForModalMetric>(edge: &E) -> Self {
        let hash = edge.id_hash();
        let id_hash = [hash as u32, (hash >> 32) as u32];
        let src_vertex_id = u32::try_from(edge.src_vertex_id()).unwrap_or(u32::MAX);
        let speed_limit_mph = edge
            .get_traffic_speed_limit()
            .and_then(|s| i16::try_from(s).ok())
            .unwrap_or(i16::MIN);
        let cycleway_tag = edge.get_cycleway_tag();

        let mut flags = 0u8;
        if edge.is_walkable() {
            flags |= Self::FLAG_WALKABLE;
        }
        if edge.is_walkable_highway() {
            flags |= Self::FLAG_WALKABLE_HIGHWAY;
        }
        if edge.is_sidewalk() {
            flags |= Self::FLAG_SIDEWALK;
        }
        if edge.is_footway() {
            flags |= Self::FLAG_FOOTWAY;
        }
        if edge.is_pedestrian_priority() {
            flags |= Self::FLAG_PEDESTRIAN_PRIORITY;
        }
        if edge.is_unbikeable() {
            flags |= Self::FLAG_UNBIKEABLE;
        }
        if edge.is_non_motorized() {
            flags |= Self::FLAG_NON_MOTORIZED;
        }
        if edge.is_oneway() {
            flags |= Self::FLAG_ONEWAY;
        }

        Self {
            id_hash,
            src_vertex_id,
            speed_limit_mph,
            cycleway_tag,
            flags,
        }
    }

    #[inline]
    pub fn id_hash(&self) -> u64 {
        (self.id_hash[0] as u64) | ((self.id_hash[1] as u64) << 32)
    }

    #[inline]
    pub fn src_vertex_id(&self) -> usize {
        self.src_vertex_id as usize
    }
}

impl EdgeForModalMetric for CompactEdgeAttrs {
    #[inline]
    fn get_traffic_speed_limit(&self) -> Option<i32> {
        (self.speed_limit_mph != i16::MIN).then_some(self.speed_limit_mph as i32)
    }

    #[inline]
    fn get_cycleway_tag(&self) -> Option<CyclewayTag> {
        self.cycleway_tag
    }

    #[inline]
    fn is_walkable(&self) -> bool {
        self.flags & Self::FLAG_WALKABLE != 0
    }

    #[inline]
    fn is_walkable_highway(&self) -> bool {
        self.flags & Self::FLAG_WALKABLE_HIGHWAY != 0
    }

    #[inline]
    fn is_sidewalk(&self) -> bool {
        self.flags & Self::FLAG_SIDEWALK != 0
    }

    #[inline]
    fn is_footway(&self) -> bool {
        self.flags & Self::FLAG_FOOTWAY != 0
    }

    #[inline]
    fn is_pedestrian_priority(&self) -> bool {
        self.flags & Self::FLAG_PEDESTRIAN_PRIORITY != 0
    }

    #[inline]
    fn is_unbikeable(&self) -> bool {
        self.flags & Self::FLAG_UNBIKEABLE != 0
    }

    #[inline]
    fn is_non_motorized(&self) -> bool {
        self.flags & Self::FLAG_NON_MOTORIZED != 0
    }

    #[inline]
    fn is_oneway(&self) -> bool {
        self.flags & Self::FLAG_ONEWAY != 0
    }
}

/// `EdgeRTreeEntry` caches the bounding box, centroid, and compact modal attributes
/// of a network edge in a 40-byte `Copy` struct with zero heap allocations.
///
/// The original edge value (including its `LineString<f32>` geometry and heap `String`
/// fields) is consumed and dropped immediately in [`EdgeRTreeEntry::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeRTreeEntry<E = ()> {
    bbox: AABB<[f32; 2]>,
    pub centroid: geo::Point<f32>,
    pub edge: CompactEdgeAttrs,
    _marker: PhantomData<fn() -> E>,
}

impl<E: SpatialEdge + EdgeForModalMetric> EdgeRTreeEntry<E> {
    pub fn new(edge: E) -> Option<Self> {
        Self::from_ref(&edge)
    }

    pub fn from_ref(edge: &E) -> Option<Self> {
        let linestring = edge.linestring()?;
        let rect = linestring.bounding_rect()?;
        let centroid = linestring.centroid()?;
        let compact = CompactEdgeAttrs::from_edge(edge);

        Some(Self {
            bbox: AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]),
            centroid,
            edge: compact,
            _marker: PhantomData,
        })
    }
}

impl<E> RTreeObject for EdgeRTreeEntry<E> {
    type Envelope = AABB<[f32; 2]>;
    #[inline]
    fn envelope(&self) -> Self::Envelope {
        self.bbox
    }
}

impl<E> PointDistance for EdgeRTreeEntry<E> {
    #[inline]
    fn distance_2(&self, point: &[f32; 2]) -> f32 {
        let query_point = geo::Point::new(point[0], point[1]);
        let distance = Euclidean.distance(self.centroid, query_point);
        distance * distance
    }
}

/// Find the neighboring edges within [`DISTANCE_RTREE_NEIGHBOR`] of the
/// query edge's centroid, excluding the query edge itself (by edge ID hash).
pub fn find_neighboring_edges<'a, E>(
    query: &EdgeRTreeEntry<E>,
    rtree: &'a rstar::RTree<EdgeRTreeEntry<E>>,
) -> Vec<&'a EdgeRTreeEntry<E>> {
    let query_id_hash = query.edge.id_hash();
    rtree
        .locate_within_distance(
            [query.centroid.x(), query.centroid.y()],
            DISTANCE_RTREE_NEIGHBOR,
        )
        .filter(|entry| entry.edge.id_hash() != query_id_hash)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_traits::vertex_for_modal_metric::{
        CompactVertexAttrs, VertexForModalMetric,
    };
    use geo::LineString;

    struct MockEdge {
        id: String,
        use_id_str: bool,
        src_vertex_id: usize,
        speed_limit: Option<i32>,
        cycleway: Option<CyclewayTag>,
        flags: [bool; 8],
        linestring: LineString<f32>,
    }

    impl SpatialEdge for MockEdge {
        fn id(&self) -> String {
            self.id.clone()
        }
        fn id_str(&self) -> Option<&str> {
            self.use_id_str.then_some(self.id.as_str())
        }
        fn linestring(&self) -> Option<&LineString<f32>> {
            Some(&self.linestring)
        }
        fn src_vertex_id(&self) -> usize {
            self.src_vertex_id
        }
    }

    impl EdgeForModalMetric for MockEdge {
        fn get_traffic_speed_limit(&self) -> Option<i32> {
            self.speed_limit
        }
        fn get_cycleway_tag(&self) -> Option<CyclewayTag> {
            self.cycleway
        }
        fn is_walkable(&self) -> bool {
            self.flags[0]
        }
        fn is_walkable_highway(&self) -> bool {
            self.flags[1]
        }
        fn is_sidewalk(&self) -> bool {
            self.flags[2]
        }
        fn is_footway(&self) -> bool {
            self.flags[3]
        }
        fn is_pedestrian_priority(&self) -> bool {
            self.flags[4]
        }
        fn is_unbikeable(&self) -> bool {
            self.flags[5]
        }
        fn is_non_motorized(&self) -> bool {
            self.flags[6]
        }
        fn is_oneway(&self) -> bool {
            self.flags[7]
        }
    }

    struct MockVertex(bool, bool);
    impl VertexForModalMetric for MockVertex {
        fn has_traffic_signals(&self) -> bool {
            self.0
        }
        fn has_stop_sign(&self) -> bool {
            self.1
        }
    }

    #[test]
    fn test_compact_sizes() {
        assert_eq!(std::mem::size_of::<CompactEdgeAttrs>(), 16);
        assert_eq!(std::mem::size_of::<EdgeRTreeEntry<()>>(), 40);
        assert_eq!(std::mem::size_of::<CompactVertexAttrs>(), 1);
    }

    #[test]
    fn test_compact_edge_attrs_isolated_bitflags_and_fields() {
        let ls = LineString::from(vec![(-105.0, 40.0), (-104.99, 40.01)]);

        // Test each of the 8 boolean flags in isolation to ensure no bit collisions
        for bit_idx in 0..8 {
            let mut flags = [false; 8];
            flags[bit_idx] = true;
            let edge = MockEdge {
                id: format!("edge-{bit_idx}"),
                use_id_str: true,
                src_vertex_id: 123_456,
                speed_limit: Some(35),
                cycleway: Some(CyclewayTag::DedicatedNoBuffer),
                flags,
                linestring: ls.clone(),
            };
            let compact = CompactEdgeAttrs::from_edge(&edge);
            assert_eq!(compact.src_vertex_id(), 123_456);
            assert_eq!(compact.get_traffic_speed_limit(), Some(35));
            assert_eq!(
                compact.get_cycleway_tag(),
                Some(CyclewayTag::DedicatedNoBuffer)
            );
            let actual_flags = [
                compact.is_walkable(),
                compact.is_walkable_highway(),
                compact.is_sidewalk(),
                compact.is_footway(),
                compact.is_pedestrian_priority(),
                compact.is_unbikeable(),
                compact.is_non_motorized(),
                compact.is_oneway(),
            ];
            assert_eq!(actual_flags, flags, "mismatch for isolated bit {bit_idx}");
        }

        // Verify None speed_limit and None cycleway_tag, and id_str vs owned id hash equivalence
        let e_borrowed = MockEdge {
            id: "same-source-id".to_string(),
            use_id_str: true,
            src_vertex_id: 0,
            speed_limit: None,
            cycleway: None,
            flags: [false; 8],
            linestring: ls.clone(),
        };
        let e_owned = MockEdge {
            id: "same-source-id".to_string(),
            use_id_str: false,
            src_vertex_id: 0,
            speed_limit: None,
            cycleway: None,
            flags: [false; 8],
            linestring: ls,
        };
        let c_borrowed = CompactEdgeAttrs::from_edge(&e_borrowed);
        let c_owned = CompactEdgeAttrs::from_edge(&e_owned);
        assert_eq!(c_borrowed.get_traffic_speed_limit(), None);
        assert_eq!(c_borrowed.get_cycleway_tag(), None);
        assert_eq!(c_borrowed.id_hash(), c_owned.id_hash());
    }

    #[test]
    fn test_compact_vertex_attrs_combinations() {
        for (sig, stop) in [(false, false), (true, false), (false, true), (true, true)] {
            let v = MockVertex(sig, stop);
            let compact = CompactVertexAttrs::from_vertex(&v);
            assert_eq!(compact.has_traffic_signals(), sig);
            assert_eq!(compact.has_stop_sign(), stop);
        }
    }
}
