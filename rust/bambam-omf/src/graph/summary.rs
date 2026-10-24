pub use bambam_network::summary::{ClassStats, EdgeListStats};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    app::CliBoundingBox, collection::OvertureMapsCollectionError, graph::OmfGraphVectorized,
};

/// summarizes an OMF import of a network.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OmfGraphSummary {
    /// information describing how this dataset was generated
    pub source: OmfGraphSource,
    /// statistics gathered on the graph dataset
    pub stats: OmfGraphStats,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct OmfGraphSource {
    /// location of imported OMF dataset. this should either be
    /// an official OMF release identifier or a local file path.
    pub release: String,
    /// user-provided name for the network
    pub study_region: String,
    /// date and time this network was created
    pub created: String,
    /// bounding box query used when run
    pub bbox: Option<CliBoundingBox>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub struct OmfGraphStats {
    /// number of vertices in the network
    pub vertices: usize,
    /// details for each edge list.
    pub edge_list: IndexMap<String, EdgeListStats>,
}

impl OmfGraphSource {
    pub fn new(release: &str, study_region: &str, bbox: Option<&CliBoundingBox>) -> Self {
        let created = chrono::Utc::now().to_rfc3339();
        Self {
            release: release.to_string(),
            study_region: study_region.to_string(),
            created,
            bbox: bbox.cloned(),
        }
    }
}

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
