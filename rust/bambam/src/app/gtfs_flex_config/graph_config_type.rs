use std::path::Path;

use bambam_gtfs_flex::{model::consts::MODE_NAME, util::zone::ZoneLookupConfig};
use config::Config;
use itertools::Itertools;
use routee_compass::app::compass::{CompassAppConfig, SearchConfig};
use routee_compass_core::{
    config::OneOrMany,
    model::network::{EdgeListConfig, GraphConfig},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    app::gtfs_flex_config::{
        run::{iter_models, vec_models},
        CliGraphConfig, GtfsFlexConfigError,
    },
    model::{
        constraint::multimodal::MultimodalConstraintConfig,
        traversal::multimodal::MultimodalTraversalConfig,
    },
};

/// the GTFS-Flex edge list configuration can either be provided 1) by copying another
/// existing edge list from the configuration, or, 2) from a file provided by the user.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GraphConfigType {
    /// index of an edge list declared in the base configuration. we will copy this
    /// configuration, removing instances of multimodal configuration which are added
    /// back later downstream.
    FromConfigEdgeList { index: usize },
    /// file containing the configuration block to inject for this edge list. can
    /// be TOML, YAML, or JSON.
    FromFile { input_file: String },
}

impl GraphConfigType {
    /// builds the graph from
    pub fn build_graph(
        &self,
        base_conf: &CompassAppConfig,
    ) -> Result<GraphConfig, GtfsFlexConfigError> {
        // build the row we want to add based on the config type used here.
        let row = match self {
            GraphConfigType::FromConfigEdgeList { index } => {
                let edge_lists = base_conf.graph.edge_list.as_vec();
                match edge_lists.get(*index) {
                    Some(el) => (*el).clone(),
                    None => {
                        let msg = format!("while updating [graph], did not find edge list at expected index {index}");
                        return Err(GtfsFlexConfigError::RunFailure(msg));
                    }
                }
            }
            GraphConfigType::FromFile { input_file } => EdgeListConfig {
                input_file: input_file.clone(),
            },
        };

        // push onto the edge lists
        let edge_list = match &base_conf.graph.edge_list {
            OneOrMany::Many(items) if items.is_empty() => OneOrMany::One(row.clone()),
            OneOrMany::Many(items) => {
                let mut updated = items.clone();
                updated.push(row.clone());
                OneOrMany::Many(updated)
            }
            OneOrMany::One(prev) => OneOrMany::Many(vec![prev.clone(), row]),
        };
        Ok(GraphConfig {
            vertex_list_input_file: base_conf.graph.vertex_list_input_file.clone(),
            edge_list,
        })
    }
}

impl TryFrom<CliGraphConfig> for GraphConfigType {
    type Error = GtfsFlexConfigError;

    fn try_from(value: CliGraphConfig) -> Result<Self, Self::Error> {
        match (value.graph_edge_list, value.graph_edge_list_input_file) {
            (None, Some(input_file)) => Ok(Self::FromFile { input_file }),
            (Some(index), None) => Ok(Self::FromConfigEdgeList { index }),
            _ => Err(GtfsFlexConfigError::InternalError(
                "one graph config should exist".to_string(),
            )),
        }
    }
}
