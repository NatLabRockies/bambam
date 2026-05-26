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
        CliGtfsFlexConfig, GtfsFlexConfigError,
    },
    model::{
        constraint::multimodal::MultimodalConstraintConfig,
        traversal::multimodal::MultimodalTraversalConfig,
    },
};

/// the GTFS-Flex edge list configuration can either be provided 1) by copying another
/// existing edge list from the configuration, or, 2) from a file provided by the user.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GtfsFlexConfigType {
    /// index of an edge list declared in the base configuration. we will copy this
    /// configuration, removing instances of multimodal configuration which are added
    /// back later downstream.
    FromConfigEdgeList { index: usize },
    /// file containing the configuration block to inject for this edge list. can
    /// be TOML, YAML, or JSON.
    FromFile { input_file: String },
}

impl GtfsFlexConfigType {
    /// given some base SearchConfig, a directory with (processed) GTFS-Flex data, and the updated list
    /// of available modes, construct the edge list of SearchConfig data for a GTFS-Flex configuration.
    pub fn build_flex_search_config(
        &self,
        base: &[SearchConfig],
        flex_dir: &str,
        available_modes: &[String],
    ) -> Result<SearchConfig, GtfsFlexConfigError> {
        // set up the GTFS-Flex configurations, both a ZoneLookupConfig with a "type": "gtfs-flex"
        let flex_path = Path::new(flex_dir);
        let flex_conf = ZoneLookupConfig::from(flex_path);
        let mut flex_cm = json!(flex_conf.clone());
        flex_cm["type"] = json!(MODE_NAME);
        let mut flex_tm = json!(flex_conf.clone());
        flex_tm["type"] = json!(MODE_NAME);

        match self {
            GtfsFlexConfigType::FromConfigEdgeList { index } => {
                // copy over existing edge list data here
                let edge_list: &SearchConfig = base.get(*index)
                    .ok_or_else(|| {
                        let msg = format!("given index '{index}' for underlying edge list does not exist in source config");
                        GtfsFlexConfigError::RunFailure(msg)
                    })?;
                inject_gtfs_flex_models(edge_list, flex_dir, available_modes)
            }
            GtfsFlexConfigType::FromFile { input_file } => {
                let config_file = config::File::with_name(input_file);
                let config_raw =
                    Config::builder()
                        .add_source(config_file)
                        .build()
                        .map_err(|source| {
                            let msg = format!(
                                "failure reading '{input_file}' as a valid Config (TOML|YAML|JSON)"
                            );
                            GtfsFlexConfigError::ConfigReadFailure { msg, source }
                        })?;
                let config: SearchConfig = config_raw.try_deserialize().map_err(|source| {
                    let msg = format!(
                        "failure reading '{input_file}' contents as a Compass SearchConfig"
                    );
                    GtfsFlexConfigError::ConfigReadFailure { msg, source }
                })?;
                inject_gtfs_flex_models(&config, flex_dir, available_modes)
            }
        }
    }
}

impl TryFrom<CliGtfsFlexConfig> for GtfsFlexConfigType {
    type Error = GtfsFlexConfigError;

    fn try_from(value: CliGtfsFlexConfig) -> Result<Self, Self::Error> {
        match (
            value.gtfs_flex_edge_list,
            value.gtfs_flex_search_config_input_file,
        ) {
            (None, Some(input_file)) => Ok(Self::FromFile { input_file }),
            (Some(index), None) => Ok(Self::FromConfigEdgeList { index }),
            _ => Err(GtfsFlexConfigError::InternalError(
                "one GTFS-Flex config should exist".to_string(),
            )),
        }
    }
}

fn remove_multimodal_model(obj: &Value) -> bool {
    match obj.get("type") {
        Some(t) => match t.as_str() {
            Some(t_str) => t_str != "multimodal",
            None => true,
        },
        None => true,
    }
}

/// injects traversal models for gtfs-flex and multimodal search into the incoming
/// SearchConfig (after first removing any existing multimodal config which would
/// have out-of-date available modes listing).
fn inject_gtfs_flex_models(
    conf: &SearchConfig,
    flex_dir: &str,
    available_modes: &[String],
) -> Result<SearchConfig, GtfsFlexConfigError> {
    // setup for multimodal models
    let mmcm = MultimodalConstraintConfig {
        this_mode: MODE_NAME.to_string(),
        available_modes: available_modes.to_vec(),
    };
    let mmtm = MultimodalTraversalConfig {
        this_mode: MODE_NAME.to_string(),
        available_modes: available_modes.to_vec(),
    };

    // setup for GTFS-Flex models
    let flex_dir = Path::new(flex_dir);
    let flex_conf = ZoneLookupConfig::from(flex_dir);
    let mut flex_cm = json!(flex_conf.clone());
    flex_cm["type"] = json!(MODE_NAME);
    let mut flex_tm = json!(flex_conf.clone());
    flex_tm["type"] = json!(MODE_NAME);

    let mut constraint = vec_models(&conf.constraint)?
        .into_iter()
        .filter(remove_multimodal_model)
        .collect_vec();
    let mut traversal = vec_models(&conf.traversal)?
        .into_iter()
        .filter(remove_multimodal_model)
        .collect_vec();

    constraint.push(flex_cm);
    constraint.push(json!(mmcm));
    traversal.push(flex_tm);
    traversal.push(json!(mmtm));

    Ok(SearchConfig {
        traversal: json!(traversal),
        constraint: json!(constraint),
    })
}
