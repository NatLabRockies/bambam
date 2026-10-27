pub use bambam_network::summary::{
    ClassStats, EdgeListStats, NetworkGraphSource as OmfGraphSource,
    NetworkGraphStats as OmfGraphStats, NetworkGraphSummary as OmfGraphSummary,
};
use indexmap::IndexMap;

use crate::{collection::OvertureMapsCollectionError, graph::OmfGraphVectorized};

impl TryFrom<&OmfGraphVectorized> for OmfGraphStats {
    type Error = OvertureMapsCollectionError;

    fn try_from(value: &OmfGraphVectorized) -> Result<Self, Self::Error> {
        let edge_list_iter = value.edge_list_config.iter().zip(value.edge_lists.iter());
        let mut edge_list = IndexMap::new();
        for (c, e) in edge_list_iter {
            let key = c.mode.clone();
            let stats = EdgeListStats::from_edge_list(e, |cls| cls.as_str());
            let _ = edge_list.insert(key, stats);
        }
        Ok(OmfGraphStats {
            vertices: value.vertices.len(),
            edge_list,
        })
    }
}
