use std::path::Path;

use bambam_gtfs_flex::{model::consts::MODE_NAME, util::zone::ZoneLookupConfig};
use config::Config;
use itertools::Itertools;
use routee_compass::app::compass::{CompassAppConfig, SearchConfig};
use routee_compass_core::{
    config::OneOrMany,
    model::{
        map::{MapModelConfig, MapModelGeometryConfig},
        network::{EdgeListConfig, GraphConfig},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    app::gtfs_flex_config::{
        run::{iter_models, vec_models},
        CliMappingConfig, GtfsFlexConfigError,
    },
    model::{
        constraint::multimodal::MultimodalConstraintConfig,
        traversal::multimodal::MultimodalTraversalConfig,
    },
};

/// the GTFS-Flex edge list configuration can either be provided 1) by copying another
/// existing edge list from the configuration, or, 2) from a file provided by the user.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum MapConfigType {
    /// index of an edge list declared in the base configuration. we will copy this
    /// configuration, removing instances of multimodal configuration which are added
    /// back later downstream.
    FromConfigEdgeList { index: usize },
    /// file containing the configuration block to inject for this edge list. can
    /// be TOML, YAML, or JSON.
    FromFile { input_file: String },
}

impl MapConfigType {
    /// builds the graph from
    pub fn build_map_config(
        &self,
        base_conf: &CompassAppConfig,
    ) -> Result<MapModelConfig, GtfsFlexConfigError> {
        // build the row we want to add based on the config type used here.
        let geom_conf = match self {
            MapConfigType::FromConfigEdgeList { index } => {
                let geometries = base_conf.mapping.geometry.as_vec();
                match geometries.get(*index) {
                    Some(el) => (*el).clone(),
                    None => {
                        let msg = format!("while updating [graph], did not find edge list at expected index {index}");
                        return Err(GtfsFlexConfigError::RunFailure(msg));
                    }
                }
            }
            MapConfigType::FromFile { input_file } => MapModelGeometryConfig::FromLinestrings {
                geometry_input_file: input_file.clone(),
            },
        };

        let geometry = match &base_conf.mapping.geometry {
            OneOrMany::Many(items) if items.is_empty() => OneOrMany::One(geom_conf.clone()),
            OneOrMany::Many(items) => {
                let mut updated = items.clone();
                updated.push(geom_conf.clone());
                OneOrMany::Many(updated)
            }
            OneOrMany::One(prev) => OneOrMany::Many(vec![prev.clone(), geom_conf]),
        };
        let mut result = base_conf.mapping.clone();
        result.geometry = geometry;
        Ok(result)
    }
}

impl TryFrom<CliMappingConfig> for MapConfigType {
    type Error = GtfsFlexConfigError;

    fn try_from(value: CliMappingConfig) -> Result<Self, Self::Error> {
        match (value.map_edge_list, value.map_geometries_input_file) {
            (None, Some(input_file)) => Ok(Self::FromFile { input_file }),
            (Some(index), None) => Ok(Self::FromConfigEdgeList { index }),
            _ => Err(GtfsFlexConfigError::InternalError(
                "one mapping config should exist".to_string(),
            )),
        }
    }
}
