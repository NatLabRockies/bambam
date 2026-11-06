use std::{
    fs::{DirEntry, File},
    path::{Path, PathBuf},
};

use bambam_transit::{
    model::traversal::transit::{ScheduleLoadingPolicy, TransitTraversalConfig},
    schedule::ScheduleOutputFormat,
};
use csv::QuoteStyle;
use flate2::{write::GzEncoder, Compression};
use itertools::Itertools;
use kdam::tqdm;
use regex::Regex;
use routee_compass::app::compass::{CompassAppConfig, SearchConfig};
use routee_compass_core::{
    config::OneOrMany,
    model::{
        map::MapModelGeometryConfig,
        network::{EdgeListConfig, EdgeListId},
        traversal::default::distance::DistanceTraversalConfig,
        unit::DistanceUnit,
    },
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;

use crate::{
    app::gtfs_config::gtfs_config_error::GtfsConfigError,
    model::{
        constraint::{
            multimodal::MultimodalConstraintConfig, time_limit::TimeLimitConstraintConfig,
        },
        traversal::multimodal::MultimodalTraversalConfig,
    },
};

pub const METADATA_FILENAME_REGEX: &str = r#"^edges-gtfs-metadata(?:-(\d+))?\.json$"#;
pub const FQ_ROUTE_IDS_FILENAME: &str = "fq-route-ids-enumerated.txt.gz";
pub const TRANSIT_MODE: &str = "transit";

pub fn edges_filename(edge_list_id: Option<EdgeListId>) -> String {
    match edge_list_id {
        Some(id) => format!("edges-compass-{id}.csv.gz"),
        None => "edges-compass.csv.gz".to_string(),
    }
}

pub fn schedules_csv_filename(edge_list_id: Option<EdgeListId>) -> String {
    match edge_list_id {
        Some(id) => format!("edges-schedules-{id}.csv.gz"),
        None => "edges-schedules.csv.gz".to_string(),
    }
}

pub fn schedules_rkyv_filename(edge_list_id: Option<EdgeListId>) -> String {
    match edge_list_id {
        Some(id) => format!("edges-schedules-{id}.rkyv"),
        None => "edges-schedules.rkyv".to_string(),
    }
}

pub fn schedules_filename(
    edge_list_id: Option<EdgeListId>,
    format: ScheduleOutputFormat,
) -> String {
    match format {
        ScheduleOutputFormat::Csv => schedules_csv_filename(edge_list_id),
        ScheduleOutputFormat::Rkyv => schedules_rkyv_filename(edge_list_id),
    }
}

pub fn geometries_filename(edge_list_id: Option<EdgeListId>) -> String {
    match edge_list_id {
        Some(id) => format!("edges-geometries-enumerated-{id}.txt.gz"),
        None => "edges-geometries-enumerated.txt.gz".to_string(),
    }
}

pub fn metadata_filename(edge_list_id: Option<EdgeListId>) -> String {
    match edge_list_id {
        Some(id) => format!("edges-gtfs-metadata-{id}.json"),
        None => "edges-gtfs-metadata.json".to_string(),
    }
}

/// executes a run of the GTFS configuration application.
///
/// the algorithm here can be seen as the following steps:
///   1. read in some configuration file that doesn't have transit added
///   2. copy some data from this config to duplicate across transit additions
///   3. look for all gtfs metadata JSON files in a directory
///   4. for each metadata file, look for the other expected files associated with the same edge list
///   5. for each edge list bundle, inject graph, mapping, and search configuration
///   6. re-write as a TOML file to the file system
pub fn run(
    directory: &str,
    base_config_filepath: &str,
    base_config_relative_path: Option<&str>,
) -> Result<(), GtfsConfigError> {
    // we will load and modify the base TOML configuration file. in particular,
    // we are modifying the `[[graph.edge_list]]` and `[[search]]` sections.
    let mut compass_conf: CompassAppConfig =
        CompassAppConfig::try_from(Path::new(base_config_filepath)).map_err(|e| {
            GtfsConfigError::ReadFailure {
                filepath: base_config_filepath.to_string(),
                error: e.to_string(),
            }
        })?;

    // temporary collections to modify when updating the base config
    let mut conf_graph_edge_lists: Vec<EdgeListConfig> =
        compass_conf.graph.edge_list.iter().cloned().collect_vec();
    let mut conf_geometries: Vec<MapModelGeometryConfig> =
        compass_conf.mapping.geometry.iter().cloned().collect_vec();

    // finds the travel modes that are already present in the config's edge lists.
    // ensure "transit" is one of the options and update [label].modes and existing [[search]] entries.
    let mut available_modes = get_available_modes(&compass_conf)?;
    let transit_mode = TRANSIT_MODE.to_string();
    if !available_modes.contains(&transit_mode) {
        available_modes.push(transit_mode);
    }
    update_label_modes(&mut compass_conf, &available_modes)?;
    let mut conf_search: Vec<SearchConfig> =
        update_existing_search_modes(&compass_conf, &available_modes)?;

    // grab optional time_limit constraint configuration to copy into each GTFS constraint model
    let time_limit_conf = get_time_limit_constraint_config(&compass_conf)?;

    log::info!("finding metadata files in {directory}");
    let read_dir = std::fs::read_dir(directory).map_err(|e| GtfsConfigError::ReadFailure {
        filepath: directory.to_string(),
        error: e.to_string(),
    })?;
    let metadata_file_pattern = Regex::new(METADATA_FILENAME_REGEX).map_err(|e| {
        GtfsConfigError::InternalError(format!("failure building metadata filename regex: {e}"))
    })?;
    let metadata_files: Vec<DirEntry> = read_dir
        .filter(|entry| entry_matches_pattern(entry, &metadata_file_pattern))
        .try_collect()
        .map_err(|e| GtfsConfigError::ReadFailure {
            filepath: directory.to_string(),
            error: e.to_string(),
        })?;

    // confirm all files are found related to an edge list and create a record for each edge list entry
    let mut entries: Vec<GtfsEdgeListEntry> = metadata_files
        .into_iter()
        .map(|metadata_file| {
            let edge_list_id = get_edge_list_id(&metadata_file, &metadata_file_pattern)?;
            GtfsEdgeListEntry::new(
                edge_list_id,
                directory,
                base_config_relative_path.unwrap_or_default(),
            )
        })
        .try_collect()?;
    entries.sort_by_cached_key(|e| e.edge_list_id);

    if entries.is_empty() {
        return Err(GtfsConfigError::RunFailure(format!(
            "no metadata files found in directory {directory}"
        )));
    }

    let fq_route_ids_filepath = if entries
        .iter()
        .any(|e| e.schedule_format == ScheduleOutputFormat::Csv)
    {
        Some(write_fq_route_id_file(directory, &entries)?)
    } else {
        None
    };

    let n_entries = entries.len();
    let entries_iter = tqdm!(
        entries.into_iter(),
        total = n_entries,
        desc = "processing GTFS edge lists"
    );
    log::info!("found {n_entries} metadata files.");

    for entry in entries_iter {
        // update [[graph.edge_list]]
        let edge_list_config = EdgeListConfig {
            input_file: entry.edges_input_file.to_string_lossy().to_string(),
        };
        conf_graph_edge_lists.push(edge_list_config);

        // update [[mapping.geometry]]
        conf_geometries.push(MapModelGeometryConfig::FromLinestrings {
            geometry_input_file: entry.geometries_input_file.to_string_lossy().to_string(),
        });

        // step into [search] to append traversal + constraint model configurations
        let edges_schedules_path = entry.schedules_input_file.to_string_lossy().to_string();
        let edges_metadata_path = entry.metadata_input_file.to_string_lossy().to_string();
        let tm_conf = gtfs_traversal_model_config(
            &edges_schedules_path,
            &edges_metadata_path,
            entry.schedule_format,
            &available_modes,
            fq_route_ids_filepath.as_deref(),
        )?;
        let cm_conf = gtfs_constraint_model_config(&available_modes, time_limit_conf.as_ref())?;
        conf_search.push(SearchConfig {
            traversal: tm_conf,
            constraint: cm_conf,
        });
    }

    // update base configuration and write to output file
    compass_conf.graph.edge_list = OneOrMany::Many(conf_graph_edge_lists);
    compass_conf.search = OneOrMany::Many(conf_search);
    compass_conf.mapping.geometry = OneOrMany::Many(conf_geometries);

    let result_conf = toml::to_string_pretty(&compass_conf).map_err(|e| {
        GtfsConfigError::RunFailure(format!(
            "failed to convert temporary configuration back to TOML string: {e}"
        ))
    })?;

    let conf_dir = Path::new(&base_config_filepath).parent().ok_or_else(|| {
        GtfsConfigError::RunFailure(
            "base_config_filepath argument is invalid, has no 'parent'.".to_string(),
        )
    })?;
    let mut out_filename = Path::new(base_config_filepath)
        .file_stem()
        .ok_or_else(|| {
            GtfsConfigError::RunFailure(format!(
                "base config filepath '{base_config_filepath}' has no file stem!"
            ))
        })?
        .to_string_lossy()
        .into_owned();
    out_filename.push_str("_gtfs.toml");

    let out_filepath = conf_dir.join(out_filename);
    std::fs::write(&out_filepath, &result_conf).map_err(|e| {
        GtfsConfigError::RunFailure(format!(
            "failure writing to {}: {e}",
            out_filepath.to_string_lossy()
        ))
    })?;

    Ok(())
}

/// collect all fully-qualified route ids as a contiguous vector for enumeration and write to disk,
/// returning the path to the new file, or an error.
fn write_fq_route_id_file(
    directory: &str,
    entries: &[GtfsEdgeListEntry],
) -> Result<PathBuf, GtfsConfigError> {
    let dir_path = Path::new(directory);
    let file_path = dir_path.join(FQ_ROUTE_IDS_FILENAME);
    let fq_route_ids: Vec<String> = entries
        .iter()
        .map(|entry| get_metadata_vec(&entry.metadata, "fq_route_ids"))
        .collect::<Result<Vec<Vec<_>>, _>>()?
        .into_iter()
        .flatten()
        .collect_vec();
    if let Some(mut writer) = create_writer(
        dir_path,
        FQ_ROUTE_IDS_FILENAME,
        false,
        QuoteStyle::Necessary,
        true,
    ) {
        for route_id in fq_route_ids.into_iter() {
            writer.serialize(&route_id).map_err(|e| {
                GtfsConfigError::RunFailure(format!(
                    "failed writing to {FQ_ROUTE_IDS_FILENAME}: {e}"
                ))
            })?;
        }
        Ok(file_path)
    } else {
        Err(GtfsConfigError::RunFailure(String::from(
            "unable to create write operation for fully-qualified route ids file",
        )))
    }
}

/// updates `[label].modes` in `compass_conf` with the complete list of `available_modes`.
pub fn update_label_modes(
    compass_conf: &mut CompassAppConfig,
    available_modes: &[String],
) -> Result<(), GtfsConfigError> {
    let label_obj = compass_conf.label.as_object_mut().ok_or_else(|| {
        GtfsConfigError::RunFailure("label model configuration is not a JSON object".to_string())
    })?;
    label_obj.insert("modes".to_string(), json!(available_modes));
    Ok(())
}

/// updates all existing `[[search]]` entries so their `multimodal` traversal and constraint models
/// reflect `available_modes` (including `"transit"`).
pub fn update_existing_search_modes(
    base_conf: &CompassAppConfig,
    available_modes: &[String],
) -> Result<Vec<SearchConfig>, GtfsConfigError> {
    let mut updated_searches = Vec::with_capacity(base_conf.search.len());
    for (edge_list_id, search) in base_conf.search.iter().enumerate() {
        let mut traversal = search.traversal.clone();
        if let Some(models) = traversal.get_mut("models").and_then(|m| m.as_array_mut()) {
            for model in models.iter_mut() {
                if model.get("type").and_then(|t| t.as_str()) == Some("multimodal") {
                    if let Some(obj) = model.as_object_mut() {
                        obj.insert("available_modes".to_string(), json!(available_modes));
                    }
                }
            }
        } else {
            return Err(GtfsConfigError::RunFailure(format!(
                "key 'models' missing or not an array in traversal configuration for edge list {edge_list_id}"
            )));
        }

        let mut constraint = search.constraint.clone();
        if let Some(models) = constraint.get_mut("models").and_then(|m| m.as_array_mut()) {
            for model in models.iter_mut() {
                if model.get("type").and_then(|t| t.as_str()) == Some("multimodal") {
                    if let Some(obj) = model.as_object_mut() {
                        obj.insert("available_modes".to_string(), json!(available_modes));
                    }
                }
            }
        } else {
            return Err(GtfsConfigError::RunFailure(format!(
                "key 'models' missing or not an array in constraint configuration for edge list {edge_list_id}"
            )));
        }

        updated_searches.push(SearchConfig {
            traversal,
            constraint,
        });
    }
    Ok(updated_searches)
}

/// extracts the `time_limit` constraint configuration from the first edge list if present,
/// so it can be copied onto the transit constraint model.
pub fn get_time_limit_constraint_config(
    base_conf: &CompassAppConfig,
) -> Result<Option<TimeLimitConstraintConfig>, GtfsConfigError> {
    if let Some((edge_list_id, search)) = base_conf.search.iter().enumerate().next() {
        let models = search.constraint.get("models").ok_or_else(|| {
            GtfsConfigError::RunFailure(format!(
                "key 'models' missing from constraint model configuration in edge list {edge_list_id}"
            ))
        })?;
        let models_vec = models.as_array().ok_or_else(|| {
            GtfsConfigError::RunFailure(format!(
                "constraint model key 'models' in edge list {edge_list_id} is not an array"
            ))
        })?;
        if models_vec
            .iter()
            .any(|c| c.get("type").and_then(|t| t.as_str()) == Some("time_limit"))
        {
            let tlc: TimeLimitConstraintConfig =
                find_expected_config(models_vec, EdgeListId(edge_list_id), "time_limit")?;
            return Ok(Some(tlc));
        }
    }
    Ok(None)
}

/// helper function for finding a deserializable configuration within a list of JSON values.
pub fn find_expected_config<T>(
    models: &[serde_json::Value],
    edge_list_id: EdgeListId,
    expected_name: &str,
) -> Result<T, GtfsConfigError>
where
    T: DeserializeOwned,
{
    let model_conf = models
        .iter()
        .find(|c| {
            if let Some(t_val) = c.get("type") {
                t_val.as_str() == Some(expected_name)
            } else {
                false
            }
        })
        .ok_or_else(|| {
            GtfsConfigError::RunFailure(format!(
                "edge list {edge_list_id} has no '{expected_name}' model"
            ))
        })?;
    let mut conf_clean = model_conf.clone();
    let failure_deleting_type = match conf_clean.as_object_mut() {
        Some(obj) => obj.remove("type").is_none(),
        None => {
            return Err(GtfsConfigError::InternalError(format!(
                "after keying on 'type', was unable to delete the key in JSON: \n{}",
                serde_json::to_string_pretty(model_conf).unwrap_or_default()
            )));
        }
    };
    if failure_deleting_type {
        return Err(GtfsConfigError::InternalError(format!(
            "failed while removing 'type' key, did not find in object: \n {}",
            serde_json::to_string_pretty(&model_conf).unwrap_or_default(),
        )));
    }
    let result: T = serde_json::from_value(conf_clean).map_err(|e| {
        GtfsConfigError::RunFailure(format!(
            "failed to parse '{expected_name}' model config for edge list {edge_list_id}: {e}. JSON:\n{}",
            serde_json::to_string_pretty(model_conf).unwrap_or_default()
        ))
    })?;
    Ok(result)
}

/// finds what modes are already available via other edge lists via the Label model in the config.
/// assumes that each edge list has a "multimodal" TraversalModel type.
/// enforces that the mode list matches the listing in the label model.
pub fn get_available_modes(base_conf: &CompassAppConfig) -> Result<Vec<String>, GtfsConfigError> {
    let lm_modes: Vec<String> = base_conf
        .label
        .get("modes")
        .ok_or_else(|| {
            GtfsConfigError::RunFailure("label model does not have a 'modes' key".to_string())
        })?
        .as_array()
        .ok_or_else(|| {
            GtfsConfigError::RunFailure(
                "label model 'modes' key does not have an array value".to_string(),
            )
        })?
        .iter()
        .enumerate()
        .map(|(idx, v)| {
            let v_str = v.as_str().ok_or_else(|| {
                GtfsConfigError::RunFailure(format!(
                    "label model '.modes[{idx}]' value is not a string"
                ))
            })?;
            Ok(v_str.to_string())
        })
        .try_collect()?;
    Ok(lm_modes)
}

/// get a vector of strings from the metadata object by some key.
pub fn get_metadata_vec(
    metadata: &serde_json::Value,
    key: &str,
) -> Result<Vec<String>, GtfsConfigError> {
    let vec_of_values = metadata
        .get(key)
        .ok_or_else(|| GtfsConfigError::RunFailure(format!("metadata missing '{key}' key")))?;
    let vec_of_strings: Vec<String> =
        serde_json::from_value(vec_of_values.clone()).map_err(|e| {
            GtfsConfigError::RunFailure(format!("metadata '{key}' is not an array of string: {e}"))
        })?;
    Ok(vec_of_strings)
}

/// generates the JSON fields expected for a transit traversal model
pub fn gtfs_traversal_model_config(
    edges_schedules: &str,
    edges_metadata: &str,
    schedule_format: ScheduleOutputFormat,
    available_modes: &[String],
    fq_route_ids_filepath: Option<&Path>,
) -> Result<serde_json::Value, GtfsConfigError> {
    let dtc_conf = DistanceTraversalConfig {
        distance_unit: Some(DistanceUnit::Miles),
        include_trip_distance: Some(true),
    };
    let ttc_conf = match schedule_format {
        ScheduleOutputFormat::Rkyv => TransitTraversalConfig::Rkyv {
            timetable_input_file: edges_schedules.to_string(),
            walk_speed: Default::default(),
        },
        ScheduleOutputFormat::Csv => {
            let route_ids_input_file =
                fq_route_ids_filepath.map(|p| p.to_string_lossy().to_string());
            TransitTraversalConfig::Csv {
                edges_schedules_input_file: edges_schedules.to_string(),
                gtfs_metadata_input_file: edges_metadata.to_string(),
                schedule_loading_policy: ScheduleLoadingPolicy::All,
                route_ids_input_file,
                walk_speed: Default::default(),
            }
        }
    };
    let mtc_conf = MultimodalTraversalConfig {
        this_mode: TRANSIT_MODE.to_string(),
        available_modes: available_modes.to_vec(),
    };
    let dtc = as_json_with_type_tag(&dtc_conf, "distance")?;
    let ttc = as_json_with_type_tag(&ttc_conf, "transit")?;
    let mtc = as_json_with_type_tag(&mtc_conf, "multimodal")?;

    let result = json![{
        "type": "combined",
        "models": [dtc, ttc, mtc]
    }];
    Ok(result)
}

/// generates the JSON fields expected for a transit constraint model
pub fn gtfs_constraint_model_config(
    available_modes: &[String],
    time_limit_conf: Option<&TimeLimitConstraintConfig>,
) -> Result<serde_json::Value, GtfsConfigError> {
    let mut models = Vec::new();
    if let Some(tlc_conf) = time_limit_conf {
        let tlc = as_json_with_type_tag(tlc_conf, "time_limit")?;
        models.push(tlc);
    }
    let mmc_conf = MultimodalConstraintConfig {
        this_mode: TRANSIT_MODE.to_string(),
        available_modes: available_modes.to_vec(),
    };
    let mmc = as_json_with_type_tag(&mmc_conf, "multimodal")?;
    models.push(mmc);

    let result = json![{
        "type": "combined",
        "models": models
    }];
    Ok(result)
}

pub struct GtfsEdgeListEntry {
    pub edge_list_id: EdgeListId,
    pub edges_input_file: PathBuf,
    pub schedules_input_file: PathBuf,
    pub schedule_format: ScheduleOutputFormat,
    pub geometries_input_file: PathBuf,
    pub metadata_input_file: PathBuf,
    pub metadata: serde_json::Value,
}

impl GtfsEdgeListEntry {
    pub fn new(
        edge_list_id: Option<EdgeListId>,
        gtfs_edge_list_directory: &str,
        relative_path_to_gtfs_edge_list_directory: &str,
    ) -> Result<GtfsEdgeListEntry, GtfsConfigError> {
        let path =
            Path::new(relative_path_to_gtfs_edge_list_directory).join(gtfs_edge_list_directory);
        let edges_filename = edges_filename(edge_list_id);
        let edges_filepath = path.join(edges_filename);
        let rkyv_schedules_filepath = path.join(schedules_rkyv_filename(edge_list_id));
        let csv_schedules_filepath = path.join(schedules_csv_filename(edge_list_id));
        let (schedules_filepath, schedule_format) = if rkyv_schedules_filepath.is_file() {
            (rkyv_schedules_filepath, ScheduleOutputFormat::Rkyv)
        } else {
            (csv_schedules_filepath, ScheduleOutputFormat::Csv)
        };
        let geometries_filename = geometries_filename(edge_list_id);
        let geometries_filepath = path.join(geometries_filename);
        let metadata_filename = metadata_filename(edge_list_id);
        let metadata_filepath = path.join(metadata_filename);
        let resolved_edge_list_id = edge_list_id.unwrap_or(EdgeListId(0));
        if !&edges_filepath.is_file() {
            Err(GtfsConfigError::ReadFailure {
                filepath: edges_filepath.to_string_lossy().to_string(),
                error: "file not found".to_string(),
            })
        } else if !&schedules_filepath.is_file() {
            Err(GtfsConfigError::ReadFailure {
                filepath: schedules_filepath.to_string_lossy().to_string(),
                error: "file not found".to_string(),
            })
        } else if !&geometries_filepath.is_file() {
            Err(GtfsConfigError::ReadFailure {
                filepath: geometries_filepath.to_string_lossy().to_string(),
                error: "file not found".to_string(),
            })
        } else if !&metadata_filepath.is_file() {
            Err(GtfsConfigError::ReadFailure {
                filepath: metadata_filepath.to_string_lossy().to_string(),
                error: "file not found".to_string(),
            })
        } else {
            let metadata_string = std::fs::read_to_string(&metadata_filepath).map_err(|e| {
                GtfsConfigError::ReadFailure {
                    filepath: metadata_filepath.to_string_lossy().to_string(),
                    error: e.to_string(),
                }
            })?;
            let metadata: serde_json::Value =
                serde_json::from_str(&metadata_string).map_err(|e| {
                    GtfsConfigError::ReadFailure {
                        filepath: metadata_filepath.to_string_lossy().to_string(),
                        error: e.to_string(),
                    }
                })?;
            let entry = GtfsEdgeListEntry {
                edge_list_id: resolved_edge_list_id,
                edges_input_file: edges_filepath,
                schedules_input_file: schedules_filepath,
                schedule_format,
                geometries_input_file: geometries_filepath,
                metadata_input_file: metadata_filepath,
                metadata,
            };
            Ok(entry)
        }
    }
}

/// helper function to handle
///   1. if the entry is Ok(_), test if it's filename matches the pattern
///   2. if the entry is Err(_), return true (keep the error to fail at end of combinator)
fn entry_matches_pattern(entry: &Result<DirEntry, std::io::Error>, pat: &Regex) -> bool {
    entry
        .as_ref()
        .map(|e| {
            let filename_os = e.file_name();
            let filename = filename_os.to_string_lossy();
            pat.is_match(&filename)
        })
        .unwrap_or(true)
}

/// helper function to extract the optional EdgeListId enumerated in a metadata filename
fn get_edge_list_id(entry: &DirEntry, pat: &Regex) -> Result<Option<EdgeListId>, GtfsConfigError> {
    let filename_os = entry.file_name();
    let filename = filename_os.to_string_lossy();
    let caps = pat.captures(&filename).ok_or_else(|| {
        GtfsConfigError::InternalError(format!(
            "while extracting EdgeListId, file {filename} does not match pattern"
        ))
    })?;
    match caps.get(1) {
        Some(pat_match) => {
            let edge_list_id = pat_match.as_str().parse::<usize>().map_err(|e| {
                GtfsConfigError::InternalError(format!(
                    "while extracting EdgeListId, value {} was not a valid usize: {}",
                    pat_match.as_str(),
                    e
                ))
            })?;
            Ok(Some(EdgeListId(edge_list_id)))
        }
        None => Ok(None),
    }
}

/// helper function to build a filewriter for writing either .csv.gz or
/// .txt.gz files for compass datasets while respecting the user's overwrite
/// preferences and properly formatting WKT outputs.
fn create_writer(
    directory: &Path,
    filename: &str,
    has_headers: bool,
    quote_style: QuoteStyle,
    overwrite: bool,
) -> Option<csv::Writer<GzEncoder<File>>> {
    let filepath = directory.join(filename);
    if filepath.exists() && !overwrite {
        return None;
    }
    let file = File::create(filepath).unwrap();
    let buffer = GzEncoder::new(file, Compression::default());
    let writer = csv::WriterBuilder::new()
        .has_headers(has_headers)
        .quote_style(quote_style)
        .from_writer(buffer);
    Some(writer)
}

/// helper function that serializes the value T and then adds a "type" field to the object.
fn as_json_with_type_tag<T>(value: &T, type_tag: &str) -> Result<serde_json::Value, GtfsConfigError>
where
    T: Serialize,
{
    let mut value_json = json!(value);
    match value_json.as_object_mut() {
        Some(obj) => obj.insert("type".to_string(), json!(type_tag)),
        None => {
            return Err(GtfsConfigError::InternalError(
                "can only call as_json_with_type_tag on JSON Objects ({})".to_string(),
            ))
        }
    };
    Ok(value_json)
}
