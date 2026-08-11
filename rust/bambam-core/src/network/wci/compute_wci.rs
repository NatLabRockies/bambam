use super::ops::{
    cycleway_score, is_walk_eligible, traffic_signal_score, traffic_speed_score, walkability_score,
};
use super::wci_score::{WciError, WciScore, MAX_WCI_SCORE, MIN_WCI_SCORE};
use crate::network::network_traits::{EdgeForModalMetric, SpatialEdge, VertexForModalMetric};
use crate::network::rtree_entry::{find_neighboring_edges, EdgeRTreeEntry};
use rstar::RTree;

/// The Walking Comfort Index (WCI) scores for an edge, including the total score
/// and all components that contributed to it.
#[derive(Default)]
pub struct WciComponentScores {
    pub total_score: WciScore,
    pub walkability_score: Option<WciScore>,
    pub traffic_speed_score: Option<WciScore>,
    pub cycleway_score: Option<WciScore>,
    pub traffic_signal_score: Option<WciScore>,
}

impl WciComponentScores {
    pub fn min_wci_score() -> Result<Self, WciError> {
        Ok(Self {
            total_score: WciScore::new(MIN_WCI_SCORE)?,
            ..Default::default()
        })
    }

    pub fn max_wci_score() -> Result<Self, WciError> {
        Ok(Self {
            total_score: WciScore::new(MAX_WCI_SCORE)?,
            ..Default::default()
        })
    }
}

/// Computes the Walking Comfort Index (WCI) score for a given edge (as
/// [`EdgeRTreeEntry`]), the edge's source vertex, and the R-tree of all edges in
/// the network.
///
/// This function is generic over any map provider whose edge type implements
/// [`EdgeForModalMetric`] + [`SpatialEdge`] and whose vertex type implements
/// [`VertexForModalMetric`].
pub fn compute_wci<E, V>(
    rtree: &RTree<EdgeRTreeEntry<E>>,
    entry: &EdgeRTreeEntry<E>,
    src_vertex: &V,
) -> Result<WciComponentScores, WciError>
where
    E: EdgeForModalMetric + SpatialEdge,
    V: VertexForModalMetric,
{
    let neighboring_edges = find_neighboring_edges(entry, rtree);

    if !is_walk_eligible(entry, &neighboring_edges) {
        // Total WCI score = Min WCI score (unwalkable roadway)
        WciComponentScores::min_wci_score()
    } else if entry.edge.is_footway() || (neighboring_edges.is_empty() && entry.edge.is_sidewalk())
    {
        // Total WCI score = Max WCI score (footway or sidewalk with no adjacent edges)
        WciComponentScores::max_wci_score()
    } else {
        let walkability_score = walkability_score(&entry.edge);

        let cycleway_score = cycleway_score(entry, &neighboring_edges);

        let traffic_speed_score = traffic_speed_score(entry, &neighboring_edges);

        let traffic_signal_score = traffic_signal_score(src_vertex);

        // Total = Sum of WCI component scores
        Ok(WciComponentScores {
            total_score: &walkability_score
                + &traffic_speed_score
                + &cycleway_score
                + &traffic_signal_score,
            walkability_score: Some(walkability_score),
            traffic_speed_score: Some(traffic_speed_score),
            cycleway_score: Some(cycleway_score),
            traffic_signal_score: Some(traffic_signal_score),
        })
    }
}
