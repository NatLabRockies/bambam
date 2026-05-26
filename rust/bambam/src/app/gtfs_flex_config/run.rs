use std::io::prelude::*;
use std::{
    fs::{DirEntry, File},
    num::NonZeroU64,
    path::{Path, PathBuf},
};

use crate::{
    app::gtfs_flex_config::{
        GraphConfigType, GtfsFlexConfigError, GtfsFlexConfigType, MapConfigType,
    },
    model::{
        constraint::multimodal::{ConstraintConfig, MultimodalConstraintConfig},
        label::multimodal::{MultimodalLabelConfig, MultimodalLabelModel},
        traversal::multimodal::MultimodalTraversalConfig,
    },
};
use bambam_gtfs_flex::{
    model::{
        consts::{self, MODE_NAME},
        traversal::flex::GtfsFlexConfig,
    },
    util::zone::ZoneLookupConfig,
};
use csv::QuoteStyle;
use flate2::{write::GzEncoder, Compression};
use itertools::Itertools;
use jsonpath_rust::query::queryable::Queryable;
use kdam::tqdm;
use regex::Regex;
use routee_compass::app::compass::{CompassAppConfig, SearchConfig};
use routee_compass_core::{
    config::OneOrMany,
    model::{
        map::{MapModelConfig, MapModelGeometryConfig},
        network::{EdgeListConfig, EdgeListId, GraphConfig},
        traversal::default::distance::DistanceTraversalConfig,
        unit::DistanceUnit,
    },
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

/// executes a run of the GTFS-Flex configuration application:
///   1. append edge list (relations + geometries) onto [graph] section
///   2. inject "gtfs-flex" as a travel mode option in the label model
///   3. inject "gtfs-flex" as a travel mode option in existing multimodal configurations
///   4. create [search] configurations for GTFS-Flex
///     - GTFS-Flex constraint model
///     - GTFS-Flex traversal model
///     - Multimodal constraint model
///     - Multimodal traversal model
///
/// # Arguments
///
/// * `base_config_filepath` - path to the config file we are adding GTFS-Flex to.
/// * `output_path` - path where the result should be written, a file.
/// * `gtfs_flex_directory` - file containing processed GTFS-Flex data, generated via the bambam_gtfs_flex CLI.
/// * `graph_config` - source of the graph topology for the network used by GTFS-Flex
/// * `map_model_config` - source of the link geometries for the network used by GTFS-Flex
/// * `flex_config` - source of the traversal and constraint configuration for the GTFS-Flex edge list.
/// * `overwrite` - if true, allow overwriting the write file location.
pub fn run(
    base_config_filepath: &str,
    output_path: &str,
    gtfs_flex_directory: &str,
    graph_config: GraphConfigType,
    map_model_config: MapConfigType,
    flex_config: GtfsFlexConfigType,
    overwrite: bool,
) -> Result<(), GtfsFlexConfigError> {
    // we will load and modify the base TOML configuration file. in particular,
    // we are modifying the `[[graph.edge_list]]` and `[[search]]` sections.
    let base: CompassAppConfig = CompassAppConfig::try_from(Path::new(base_config_filepath))
        .map_err(|e| GtfsFlexConfigError::ReadFailure {
            filepath: base_config_filepath.to_string(),
            error: e.to_string(),
        })?;

    // update configuration sections
    let graph = graph_config.build_graph(&base)?;
    let mapping = map_model_config.build_map_config(&base)?;
    let label_model = updated_label_model(&base)?;
    let mut search_rows = updated_multimodal_models(&base)?;

    let gtfs_flex_search_config = flex_config.build_flex_search_config(
        &base.search.into_vec(),
        gtfs_flex_directory,
        &label_model.modes.clone().unwrap_or_default(),
    )?;

    let search = if search_rows.is_empty() {
        OneOrMany::One(gtfs_flex_search_config)
    } else {
        search_rows.push(gtfs_flex_search_config);
        OneOrMany::Many(search_rows)
    };

    let result = CompassAppConfig {
        algorithm: base.algorithm.clone(),
        state: base.state.clone(),
        cost: base.cost.clone(),
        label: json!(label_model),
        mapping,
        graph,
        search,
        plugin: base.plugin.clone(),
        termination: base.termination.clone(),
        system: base.system.clone(),
        map_matching: base.map_matching.clone(),
    };

    write_to_file(output_path, &result, overwrite)
}

fn write_to_file(
    filepath: &str,
    data: &CompassAppConfig,
    overwrite: bool,
) -> Result<(), GtfsFlexConfigError> {
    let path = Path::new(filepath);
    let file_exists = std::fs::exists(&path).map_err(|e| {
        let msg = format!("failure checking existence of {filepath}: {e}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    if file_exists {
        if overwrite {
            std::fs::remove_file(&path).map_err(|e| {
                let msg = format!("attempting to remove {filepath}: {e}",);
                GtfsFlexConfigError::RunFailure(msg)
            })?;
        } else {
            let msg = format!("'{filepath}' already exists but user did not set --overwrite",);
            return Err(GtfsFlexConfigError::RunFailure(msg));
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|e| {
            let msg = format!("cannot open '{filepath}': {e}",);
            GtfsFlexConfigError::RunFailure(msg)
        })?;
    let out = serde_json::to_string(data).map_err(|e| {
        let msg = format!("while serializing updated config, {e}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    write!(file, "{}", out).map_err(|e| {
        let msg = format!("while writing updated config to file '{filepath}', {e}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;

    Ok(())
}

/// adds the new travel mode (gtfs-flex) to the configuration files
pub fn updated_multimodal_models(
    base_conf: &CompassAppConfig,
) -> Result<Vec<SearchConfig>, GtfsFlexConfigError> {
    let mut result = vec![];
    for (edge_list_id, s) in base_conf.search.iter().enumerate() {
        let mut constraint = vec![];
        let mut found_mmcm = false;
        for c in vec_models(&s.constraint)? {
            let c_type = c.get("type");
            if matches!(Some(&json!("multimodal")), c_type) {
                found_mmcm = true;
                let mmcm = update_mmcm(&c)?;
                constraint.push(mmcm);
            } else {
                constraint.push(c)
            }
        }
        let mut traversal = vec![];
        let mut found_mmtm = false;
        for t in vec_models(&s.traversal)? {
            let t_type = t.get("type");
            if matches!(Some(&json!("multimodal")), t_type) {
                found_mmtm = true;
                let mmtm = update_mmtm(&t)?;
                traversal.push(mmtm);
            } else {
                traversal.push(t)
            }
        }
        if !found_mmcm {
            let msg = format!("MultimodalConstraintConfig not found for edge list {edge_list_id}");
            return Err(GtfsFlexConfigError::RunFailure(msg));
        }
        if !found_mmtm {
            let msg = format!("MultimodalTraversalConfig not found for edge list {edge_list_id}");
            return Err(GtfsFlexConfigError::RunFailure(msg));
        }
        result.push(SearchConfig {
            traversal: json!(traversal),
            constraint: json!(constraint),
        })
    }

    Ok(result)
}

pub fn vec_models(config: &Value) -> Result<Vec<Value>, GtfsFlexConfigError> {
    match config.get("type") {
        Some(t) if t == "combined" => {
            let models = config.get("models").ok_or_else(|| {
                let msg = String::from("combined config missing 'models' key");
                GtfsFlexConfigError::RunFailure(msg)
            })?;
            let result = models
                .as_array()
                .ok_or_else(|| {
                    GtfsFlexConfigError::RunFailure(String::from(
                        "traversal model key 'models' is not an array",
                    ))
                })?
                .clone();
            Ok(result)
        }
        Some(_) => Ok(vec![config.clone()]),
        None => {
            let msg = String::from("config has no model 'type' field");
            Err(GtfsFlexConfigError::RunFailure(msg))
        }
    }
}

pub fn iter_models<'a>(
    config: &'a Value,
) -> Result<Box<dyn Iterator<Item = &'a Value> + 'a>, GtfsFlexConfigError> {
    match config.get("type") {
        Some(t) if t == "combined" => {
            let models = config.get("models").ok_or_else(|| {
                let msg = String::from("combined config missing 'models' key");
                GtfsFlexConfigError::RunFailure(msg)
            })?;
            let result = models.as_array().ok_or_else(|| {
                GtfsFlexConfigError::RunFailure(String::from(
                    "traversal model key 'models' is not an array",
                ))
            })?;
            Ok(Box::new(result.iter()))
        }
        Some(_) => Ok(Box::new(std::iter::once(config))),
        None => {
            let msg = String::from("config has no model 'type' field");
            Err(GtfsFlexConfigError::RunFailure(msg))
        }
    }
}

pub fn update_mmcm(config: &Value) -> Result<Value, GtfsFlexConfigError> {
    let mut mmcm: MultimodalConstraintConfig = serde_json::from_value(config.clone())
        .map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))?;
    mmcm.available_modes.push(MODE_NAME.to_string());
    serde_json::to_value(mmcm).map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))
}

pub fn update_mmtm(config: &Value) -> Result<Value, GtfsFlexConfigError> {
    let mut mmcm: MultimodalTraversalConfig = serde_json::from_value(config.clone())
        .map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))?;
    mmcm.available_modes.push(MODE_NAME.to_string());
    serde_json::to_value(mmcm).map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))
}

/// finds what modes are already available via other edge lists via the Label model in the config.
/// assumes that each edge list has a "multimodal" TraversalModel type.
/// enforces that the mode list matches the listing in the label model.
pub fn updated_label_model(
    base_conf: &CompassAppConfig,
) -> Result<MultimodalLabelConfig, GtfsFlexConfigError> {
    let mut result: MultimodalLabelConfig = serde_json::from_value(base_conf.label.clone())
        .map_err(|error| GtfsFlexConfigError::LabelModelRead { error })?;

    let mut modes = match result.modes {
        Some(modes) => modes,
        None => vec![],
    };

    let has_mode = modes.iter().any(|m| m == consts::MODE_NAME);
    if !has_mode {
        modes.push(consts::MODE_NAME.to_string());
    }

    result.modes = Some(modes);

    Ok(result)
}
