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
use chrono::NaiveDateTime;
use config::Config;
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
#[allow(clippy::too_many_arguments)]
pub fn run(
    base_config_filepath: &str,
    output_path: &str,
    gtfs_flex_directory: &str,
    gtfs_flex_mode_conf_filepath: Option<&String>,
    gtfs_flex_start_time: NaiveDateTime,
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

    let gtfs_flex_mode_conf = load_gtfs_flex_mode_conf(gtfs_flex_mode_conf_filepath)?;

    // update configuration sections
    let graph = graph_config.build_graph(&base)?;
    let mapping = map_model_config.build_map_config(&base)?;
    let label_model = updated_label_model(&base)?;
    let mut search_rows = updated_multimodal_models(&base)?;
    let input_plugins =
        updated_input_plugins(&base, &gtfs_flex_mode_conf, "_modes", gtfs_flex_start_time)?;

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
    let mut plugin = base.plugin.clone();
    plugin.input_plugins = input_plugins;

    let result = CompassAppConfig {
        algorithm: base.algorithm.clone(),
        state: base.state.clone(),
        cost: base.cost.clone(),
        label: json!(label_model),
        mapping,
        graph,
        search,
        plugin,
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
    let file_exists = std::fs::exists(path).map_err(|e| {
        let msg = format!("failure checking existence of {filepath}: {e}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    if file_exists {
        if overwrite {
            std::fs::remove_file(path).map_err(|e| {
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
        .create(true)
        .open(path)
        .map_err(|e| {
            let msg = format!("cannot create output file '{filepath}': {e}",);
            GtfsFlexConfigError::RunFailure(msg)
        })?;
    let data_toml = to_toml_safe(data)?;
    write!(file, "{}", data_toml).map_err(|e| {
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
            let c_type = get_type(&c)?;
            if c_type == "multimodal" {
                found_mmcm = true;
                log::debug!("updating '{c_type}' - {c:?}");
                let mmcm = update_mmcm(&c)?;
                log::debug!("storing updated '{c_type}' - {mmcm:?}");
                constraint.push(mmcm);
            } else {
                log::debug!("storing - {c:?}");
                constraint.push(c)
            }
        }
        let mut traversal = vec![];
        let mut found_mmtm = false;
        for t in vec_models(&s.traversal)? {
            let t_type = get_type(&t)?;
            if t_type == "multimodal" {
                found_mmtm = true;
                log::debug!("updating '{t_type}' - {t:?}");
                let mmtm = update_mmtm(&t)?;
                log::debug!("storing updated '{t_type}' - {mmtm:?}");
                traversal.push(mmtm);
            } else {
                log::debug!("storing - {t:?}");
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
            traversal: json!({
                "type": "combined",
                "models": json!(traversal)
            }),
            constraint: json!({
                "type": "combined",
                "models": json!(constraint)
            }),
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
    let mut config_clean = config.clone();
    strip_type(&mut config_clean)?;
    let mut mmcm: MultimodalConstraintConfig = serde_json::from_value(config_clean)
        .map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))?;
    mmcm.available_modes.push(MODE_NAME.to_string());
    let mut result =
        serde_json::to_value(mmcm).map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))?;
    set_type(&mut result, "multimodal")?;
    Ok(result)
}

pub fn update_mmtm(config: &Value) -> Result<Value, GtfsFlexConfigError> {
    let mut config_clean = config.clone();
    strip_type(&mut config_clean)?;
    let mut mmcm: MultimodalTraversalConfig = serde_json::from_value(config_clean)
        .map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))?;
    mmcm.available_modes.push(MODE_NAME.to_string());
    let mut result =
        serde_json::to_value(mmcm).map_err(|e| GtfsFlexConfigError::RunFailure(e.to_string()))?;
    set_type(&mut result, "multimodal");
    Ok(result)
}

pub fn strip_type(config: &mut Value) -> Result<(), GtfsFlexConfigError> {
    let obj = match config.as_object_mut() {
        Some(obj) => obj,
        None => {
            let msg = format!("config value should be a JSON object but found {config:?}");
            return Err(GtfsFlexConfigError::RunFailure(msg));
        }
    };
    match obj.remove("type") {
        None => {
            let msg = "object expected to have a 'type' field: {config:?}";
            Err(GtfsFlexConfigError::RunFailure(msg.to_string()))
        }
        Some(_) => Ok(()),
    }
}

pub fn set_type(config: &mut Value, type_name: &str) -> Result<(), GtfsFlexConfigError> {
    let obj = match config.as_object_mut() {
        Some(obj) => obj,
        None => {
            let msg = format!("config value should be a JSON object but found {config:?}");
            return Err(GtfsFlexConfigError::RunFailure(msg));
        }
    };
    let _ = obj.insert("type".into(), json!(type_name));
    Ok(())
}

pub fn get_type(config: &Value) -> Result<&str, GtfsFlexConfigError> {
    let c_type = config.get("type").ok_or_else(|| {
        let msg = format!("model config missing 'type' key: {config:?}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    let type_str = c_type.as_str().ok_or_else(|| {
        let msg = format!("model config 'type' has non-string value: {config:?}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    Ok(type_str)
}

pub fn get_optional_str<'a>(
    config: &'a Value,
    key: &str,
) -> Result<Option<&'a str>, GtfsFlexConfigError> {
    let c_type = match config.get(key) {
        Some(value) => value,
        None => return Ok(None),
    };
    let type_str = c_type.as_str().ok_or_else(|| {
        let msg = format!("model config '{key}' has non-string value: {config:?}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    Ok(Some(type_str))
}

/// finds what modes are already available via other edge lists via the Label model in the config.
/// assumes that each edge list has a "multimodal" TraversalModel type.
/// enforces that the mode list matches the listing in the label model.
pub fn updated_label_model(
    base_conf: &CompassAppConfig,
) -> Result<MultimodalLabelConfig, GtfsFlexConfigError> {
    let mut result: MultimodalLabelConfig = serde_json::from_value(base_conf.label.clone())
        .map_err(|error| GtfsFlexConfigError::LabelModelRead { error })?;

    let mut modes = result.modes.unwrap_or_default();

    let has_mode = modes.iter().any(|m| m == consts::MODE_NAME);
    if !has_mode {
        modes.push(consts::MODE_NAME.to_string());
    }

    result.modes = Some(modes);

    Ok(result)
}

/// injects a grid_search mode configuration for gtfs-flex travel.
pub fn updated_input_plugins(
    base_conf: &CompassAppConfig,
    gtfs_flex_mode_configuration: &Value,
    modes_key: &str,
    start_time: NaiveDateTime,
) -> Result<Vec<Value>, GtfsFlexConfigError> {
    // find (via linear scan) the grid_search injection so we can update it∂
    let input_plugins = &base_conf.plugin.input_plugins;
    let mut grid_search_found = false;
    let mut updated = vec![];
    for (index, c) in input_plugins.iter().enumerate() {
        let c_type = get_type(c)?;
        let inject_key = get_optional_str(c, "key")?;
        if c_type == "inject" && inject_key == Some("grid_search") {
            grid_search_found = true;
            log::debug!("updating '{c_type}' - {c:?}");
            let result = update_grid_search(c, gtfs_flex_mode_configuration, modes_key)?;
            log::debug!("storing updated '{c_type}' - {result:?}");
            updated.push(result);
        } else {
            updated.push(c.clone());
        }
    }

    if !grid_search_found {
        let msg = "did not find grid_search in input plugins".to_string();
        return Err(GtfsFlexConfigError::RunFailure(msg));
    }

    // add inject plugin with start_time value
    let start_time = inject_plugin_start_time(start_time);
    updated.push(start_time);

    Ok(updated)
}

/// pushes the provided gtfs-flex mode configuration onto the grid search list of modes.
pub fn update_grid_search(
    conf: &Value,
    gtfs_flex_mode_configuration: &Value,
    modes_key: &str,
) -> Result<Value, GtfsFlexConfigError> {
    let mut result = conf.clone();
    // grid_search
    let value = result.get_mut("value").ok_or_else(|| {
        let msg = format!("inject plugin with key 'grid_search' missing 'value' field: {conf:?}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    let modes = value.get_mut(modes_key).ok_or_else(|| {
        let msg = format!(
            "inject plugin with key 'grid_search' missing 'value.{modes_key}' field: {conf:?}"
        );
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    let modes_array = modes.as_array_mut().ok_or_else(|| {
        let msg = format!("inject plugin with key 'grid_search' has 'value.{modes_key}' field that is not an array: {conf:?}");
        GtfsFlexConfigError::RunFailure(msg)
    })?;
    modes_array.push(gtfs_flex_mode_configuration.clone());
    Ok(result)
}

fn inject_plugin_start_time(start_time: NaiveDateTime) -> Value {
    json!({
        "type": "inject",
        "write_mode": "overwrite",
        "format": "key_value",
        "key": "start_time",
        "value": json!(start_time)
    })
}

/// reads the JSON file containing a gtfs-flex grid_search mode configuration. if none provided, loads the default.
fn load_gtfs_flex_mode_conf(path: Option<&String>) -> Result<Value, GtfsFlexConfigError> {
    let gtfs_flex_fp = match path {
        Some(fp) => PathBuf::from(fp),
        None => {
            // load the default configuration
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("app")
                .join("gtfs_flex_config")
                .join("default_grid_search.json")
        }
    };
    let conf_str =
        std::fs::read_to_string(&gtfs_flex_fp).map_err(|e| GtfsFlexConfigError::ReadFailure {
            filepath: gtfs_flex_fp.to_string_lossy().to_string(),
            error: e.to_string(),
        })?;
    let conf = serde_json::from_str(&conf_str).map_err(|e| GtfsFlexConfigError::ReadFailure {
        filepath: gtfs_flex_fp.to_string_lossy().to_string(),
        error: e.to_string(),
    })?;
    Ok(conf)
}

// hack to avoid complications when writing to TOML.
fn to_toml_safe(data: &CompassAppConfig) -> Result<String, GtfsFlexConfigError> {
    // convert to JSON
    let mut json_value = serde_json::to_value(data).map_err(|e| {
        GtfsFlexConfigError::RunFailure(format!("Failed to serialize to JSON Value: {e}"))
    })?;

    strip_nulls(&mut json_value);

    // JSON to TOML
    let data_toml = toml::to_string_pretty(&json_value).map_err(|e| {
        GtfsFlexConfigError::RunFailure(format!(
            "failed to convert configuration back to TOML string: {e}"
        ))
    })?;

    Ok(data_toml)
}

/// recursively strip away null values in this JSON
fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            // Remove any keys where the value is null
            map.retain(|_, v| !v.is_null());
            // Recurse into the remaining values
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        serde_json::Value::Array(arr) => {
            // Recurse into array elements
            for v in arr.iter_mut() {
                strip_nulls(v);
            }
        }
        _ => {}
    }
}
