use super::opportunity_model::OpportunityModel;
use super::opportunity_source::OpportunitySource;
use super::OpportunityDataset;
use bambam_core::model::output_plugin::opportunity::OpportunityOrientation;
use csv::{ReaderBuilder, StringRecord};
use flate2::read::GzDecoder;
use geo::{Centroid, Convert, Point};
use geozero::ToWkt;
use itertools::Itertools;
use kdam::{tqdm, Bar, BarExt};
use routee_compass::plugin::output::OutputPluginError;
use routee_compass_core::model::network::Vertex;
use routee_compass_core::util::fs::{fs_utils, read_utils};
use rstar::primitives::GeomWithData;
use rstar::RTree;
use serde::de;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs::File, io::BufReader, path::Path};

/// Configuration object for building an [`OpportunityModel`] called by it's
/// [`routee_compass::plugin::output::OutputPluginBuilder`]. See [`OpportunityModel`]
/// for algorithm implementation details.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum OpportunityModelConfig {
    /// this collection of opportunities comes from a file source.
    #[serde(rename = "file")]
    FileSource {
        opportunity_input_file: String,
        activity_column_names: Vec<String>,
        table_orientation: OpportunityOrientation,
    },
    /// This collection of opportunities comes from an API.
    ///
    /// # Fields
    /// - `vertex_input_file`: Path to CSV file containing the input vertices for the Opportunity Model.
    ///   Should match the CSV file used for constructing the traversal model graph.
    /// - `opportunity_source`: Variant of [`OpportunitySource`] describing the API to be used for opportunity collection
    /// - `activity_column_names`: Vector of String identifiers for the types of activities. E.g., ["food", "healthcare"]
    /// - `table_orientation`: Variant of [`OpportunityTableOrientation`] describing how to attach opportunities to graph elements
    #[serde(rename = "api")]
    ApiSource {
        #[serde(default)]
        vertex_input_file: String,
        opportunity_source: OpportunitySource,
        activity_column_names: Vec<String>,
        table_orientation: OpportunityOrientation,
    },
    #[serde(rename = "combined")]
    Combined {
        models: Vec<Box<OpportunityModelConfig>>,
    },
}

impl OpportunityModelConfig {
    /// Overrides `vertex_input_file` on any [`OpportunityModelConfig::ApiSource`] variants
    /// (including nested ones inside [`OpportunityModelConfig::Combined`]).
    pub fn set_vertices_file(&mut self, vertices_file: &str) {
        match self {
            OpportunityModelConfig::FileSource { .. } => {}
            OpportunityModelConfig::ApiSource {
                vertex_input_file, ..
            } => {
                *vertex_input_file = vertices_file.to_string();
            }
            OpportunityModelConfig::Combined { models } => {
                for model in models.iter_mut() {
                    model.set_vertices_file(vertices_file);
                }
            }
        }
    }

    /// Builds config into an [`OpportunityModel`]. Currently all  
    pub fn build(&self) -> Result<OpportunityModel, OutputPluginError> {
        match self {
            OpportunityModelConfig::FileSource {
                opportunity_input_file,
                activity_column_names,
                table_orientation,
            } => {
                // set up to read file
                let f = File::open(opportunity_input_file).map_err(|e| {
                    OutputPluginError::BuildFailed(format!(
                        "failed reading opportunities from {opportunity_input_file}: {e}"
                    ))
                })?;
                let r: Box<dyn std::io::Read> = if fs_utils::is_gzip(opportunity_input_file) {
                    Box::new(BufReader::new(GzDecoder::new(f)))
                } else {
                    Box::new(f)
                };
                let mut reader = ReaderBuilder::new().has_headers(true).from_reader(r);

                // track column names and their indices, and validate that all expected column names are present in the CSV header
                let mut column_lookup: HashMap<String, usize> = HashMap::new();
                reader
                    .headers()
                    .map_err(|e| {
                        OutputPluginError::BuildFailed(format!(
                            "failure reading headers from {opportunity_input_file}: {e}"
                        ))
                    })?
                    .iter()
                    .enumerate()
                    .for_each(|(index, column)| {
                        column_lookup.insert(column.to_string(), index);
                    });
                for col in activity_column_names.iter() {
                    if !column_lookup.contains_key(col) {
                        return Err(OutputPluginError::BuildFailed(format!(
                            "file {opportunity_input_file} is missing expected column {col}"
                        )));
                    }
                }

                // deserialize each CSV row, collecting the id and counts into a vector
                let mut activity_counts: Vec<Vec<f64>> = vec![];
                let rows_iter = tqdm!(
                    reader.into_records(),
                    desc = format!("opportunity source {}", opportunity_input_file)
                );
                for row_result in rows_iter {
                    let row = row_result.map_err(|e| {
                        OutputPluginError::BuildFailed(format!(
                            "failure reading row from {opportunity_input_file}: {e}"
                        ))
                    })?;
                    let mut row_counts = vec![];
                    for col in activity_column_names.iter() {
                        let cnt = get_f64_from_row(&row, col, &column_lookup)?;
                        row_counts.push(cnt);
                    }
                    activity_counts.push(row_counts);
                }

                let result = OpportunityModel::Tabular {
                    activity_types: activity_column_names.to_owned(),
                    activity_counts,
                    opportunity_orientation: table_orientation.to_owned(),
                };
                Ok(result)
            }
            OpportunityModelConfig::ApiSource {
                vertex_input_file,
                opportunity_source,
                activity_column_names,
                table_orientation,
            } => {
                let vertices = load_vertices(vertex_input_file)?;
                let raw_dataset = opportunity_source
                    .generate_dataset_with_context(activity_column_names, Some(&vertices), None)
                    .map_err(OutputPluginError::OutputPluginFailed)?;
                let activity_counts = proportion_dataset_to_vertices(
                    &raw_dataset,
                    &vertices,
                    activity_column_names.len(),
                )?;

                let result = OpportunityModel::Tabular {
                    activity_types: activity_column_names.clone(),
                    activity_counts,
                    opportunity_orientation: *table_orientation,
                };
                Ok(result)
            }
            OpportunityModelConfig::Combined { models } => {
                let models = models
                    .iter()
                    .map(|model| model.build().map(Box::new))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(OpportunityModel::Combined { models })
            }
        }
    }
}

/// Converts a `geo::Geometry<f32>` to a WKT string.
pub fn geometry_to_wkt(geometry: &geo::Geometry<f32>) -> Result<String, String> {
    let g_f64: geo::Geometry<f64> = geometry.convert();
    g_f64
        .to_wkt()
        .map_err(|e| format!("failed to convert geometry to WKT: {e}"))
}

/// Reads RouteE Compass [`Vertex`] records from `vertex_input_file` (`.csv` or `.csv.gz`).
pub fn load_vertices(vertex_input_file: &str) -> Result<Box<[Vertex]>, OutputPluginError> {
    read_utils::from_csv(
        &vertex_input_file,
        true,
        Some(
            Bar::builder()
                .desc("read vertices file for opportunity map matching")
                .animation("fillup"),
        ),
        None,
    )
    .map_err(|e| {
        OutputPluginError::BuildFailed(format!(
            "failure reading vertices from {vertex_input_file}: {e}"
        ))
    })
}

/// Assigns opportunity counts from `raw_dataset` to `vertices`:
/// - **Polygon / MultiPolygon geometries** are areal-proportioned: polygon centroids are inserted
///   into an R-tree, each vertex matches its nearest polygon centroid, and each polygon's
///   opportunity counts are divided evenly across the vertices that matched it.
/// - **Point geometries** are summed onto their nearest vertex: vertices are indexed in an R-tree,
///   each point opportunity finds its nearest vertex, and its counts are added to that vertex's row.
pub fn proportion_dataset_to_vertices(
    raw_dataset: &OpportunityDataset,
    vertices: &[Vertex],
    n_acts: usize,
) -> Result<Vec<Vec<f64>>, OutputPluginError> {
    // partition the raw dataset into point opportunities (kept as point + counts)
    // and polygonal opportunities (indexed by their centroids for R-tree lookup)
    let mut point_dataset = Vec::new();
    let mut polygon_rtree_dataset = Vec::new();

    for (index, (g, counts)) in raw_dataset.iter().enumerate() {
        if counts.len() != n_acts {
            return Err(OutputPluginError::OutputPluginFailed(format!(
                "opportunity row {index} has {} activity counts, expected {n_acts}",
                counts.len()
            )));
        }
        match g {
            geo::Geometry::Point(p) => point_dataset.push((*p, counts)),
            geo::Geometry::Polygon(p) => match p.centroid() {
                Some(centroid) => polygon_rtree_dataset.push(GeomWithData::new(centroid, index)),
                None => {
                    return Err(OutputPluginError::OutputPluginFailed(format!(
                        "opportunity source geometries must have centroids, failed with {}",
                        geometry_to_wkt(g).unwrap_or_default()
                    )))
                }
            },
            geo::Geometry::MultiPolygon(p) => match p.centroid() {
                Some(centroid) => polygon_rtree_dataset.push(GeomWithData::new(centroid, index)),
                None => {
                    return Err(OutputPluginError::OutputPluginFailed(format!(
                        "opportunity source geometries must have centroids, failed with {}",
                        geometry_to_wkt(g).unwrap_or_default()
                    )))
                }
            },
            _ => {
                return Err(OutputPluginError::OutputPluginFailed(format!(
                    "unsupported geometry, must be point, polygon, or multipolygon: {}",
                    geometry_to_wkt(g).unwrap_or_default()
                )))
            }
        }
    }

    // initialize the vertex-oriented opportunity table with zeros for each vertex
    let mut activity_counts = vec![vec![0.0; n_acts]; vertices.len()];

    // areal-proportion polygon/multipolygon opportunities across vertices that match each polygon centroid
    if !polygon_rtree_dataset.is_empty() {
        let polygon_rtree = RTree::bulk_load(polygon_rtree_dataset);

        // find which polygon centroid matches each vertex and record the dataset index of the match
        let match_iter = tqdm!(
            vertices.iter(),
            total = vertices.len(),
            desc = "map match opportunities"
        );
        let match_by_vertex = match_iter
            .map(|v| {
                let point: Point<f32> = geo::Point(v.coordinate.0).convert();
                polygon_rtree.nearest_neighbor(&point).map(|o| o.data)
            })
            .collect_vec();

        // build a lookup used for proportioning data into vertices which stores the counts
        // of vertices that match each identifier
        let proportion_groups = match_by_vertex
            .iter()
            .enumerate()
            .sorted_by_key(|(_, match_id)| *match_id)
            .chunk_by(|(_, match_id)| *match_id);
        let proportion_iter = tqdm!(
            proportion_groups.into_iter(),
            desc = "opportunities areal proportioning"
        );
        let mut proportion_lookup = HashMap::new();
        for (matching_geom_opt, matches) in proportion_iter {
            let count = matches.collect_vec().len();
            if let Some(id) = matching_geom_opt {
                proportion_lookup.insert(id, count);
            }
        }

        // divide each matched polygon's opportunity counts evenly across its matched vertices
        for (vert_idx, id_option) in match_by_vertex.iter().enumerate() {
            if let Some(id) = id_option {
                let denom = proportion_lookup.get(id).ok_or_else(|| {
                    OutputPluginError::OutputPluginFailed(String::from("internal error"))
                })?;
                let (_, raw_counts) = raw_dataset.get(*id).ok_or_else(|| {
                    OutputPluginError::OutputPluginFailed(String::from("internal error"))
                })?;
                for (acc, count) in activity_counts[vert_idx].iter_mut().zip(raw_counts.iter()) {
                    *acc += *count / *denom as f64;
                }
            }
        }
    }

    // assign each point opportunity to its nearest graph vertex and sum its counts onto that vertex
    if !point_dataset.is_empty() && !vertices.is_empty() {
        // build an R-tree of graph vertices to query the nearest vertex for each point opportunity
        let vertex_rtree_nodes = vertices
            .iter()
            .enumerate()
            .map(|(vert_idx, v)| {
                let point: Point<f32> = geo::Point(v.coordinate.0).convert();
                GeomWithData::new(point, vert_idx)
            })
            .collect_vec();
        let vertex_rtree = RTree::bulk_load(vertex_rtree_nodes);

        // accumulate each point opportunity's activity counts into its nearest vertex's row
        let point_iter = tqdm!(
            point_dataset.iter(),
            total = point_dataset.len(),
            desc = "attach point opportunities to vertices"
        );
        for (pt, raw_counts) in point_iter {
            if let Some(nearest_vert) = vertex_rtree.nearest_neighbor(pt) {
                for (acc, count) in activity_counts[nearest_vert.data]
                    .iter_mut()
                    .zip(raw_counts.iter())
                {
                    *acc += *count;
                }
            }
        }
    }

    Ok(activity_counts)
}

/// Reads a TOML or JSON configuration file from `path` into a [`serde_json::Value`].
pub fn read_config_value(path: &Path) -> Result<serde_json::Value, String> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "failed to read opportunity config from '{}': {e}",
            path.display()
        )
    })?;
    let is_json = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if is_json {
        serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse JSON config from '{}': {e}", path.display()))
    } else {
        toml::from_str(&content)
            .map_err(|e| format!("failed to parse TOML config from '{}': {e}", path.display()))
    }
}

impl TryFrom<&Path> for OpportunityModelConfig {
    type Error = String;

    /// Loads an [`OpportunityModelConfig`] from a TOML or JSON file.
    ///
    /// Supports:
    /// - A full BAMBAM config or snippet with `plugin.output_plugins` or `output_plugins` containing an `opportunity` plugin
    /// - A free-standing [`super::OpportunityPluginConfig`] containing a `model` key
    /// - A free-standing [`OpportunityModelConfig`] directly at the root
    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let value = read_config_value(path)?;
        Self::try_from(&value)
    }
}

impl TryFrom<&serde_json::Value> for OpportunityModelConfig {
    type Error = String;

    /// Extracts an [`OpportunityModelConfig`] from a parsed configuration [`serde_json::Value`].
    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        // 1. Check `plugin.output_plugins` or top-level `output_plugins`
        let output_plugins_opt = value
            .get("plugin")
            .and_then(|p| p.get("output_plugins"))
            .or_else(|| value.get("output_plugins"));

        if let Some(output_plugins) = output_plugins_opt {
            let plugins = match output_plugins {
                serde_json::Value::Array(arr) => arr.iter().collect_vec(),
                obj @ serde_json::Value::Object(_) => vec![obj],
                _ => {
                    return Err(
                        "'output_plugins' must be an array or table in configuration".to_string(),
                    )
                }
            };
            for plugin in plugins {
                let is_opp = plugin
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "opportunity")
                    .unwrap_or(false);
                if is_opp || plugin.get("model").is_some() {
                    if let Some(model_val) = plugin.get("model") {
                        return serde_json::from_value(model_val.clone()).map_err(|e| {
                            format!(
                                "failed to deserialize OpportunityModelConfig from output_plugins.model: {e}"
                            )
                        });
                    }
                }
            }
            return Err(
                "no 'opportunity' plugin with a 'model' section found in 'output_plugins'"
                    .to_string(),
            );
        }

        // 2. Check `[plugin.output_plugins.model]` table snippet without `[[plugin.output_plugins]]` array
        if let Some(model_val) = value
            .get("plugin")
            .and_then(|p| p.get("output_plugins"))
            .and_then(|op| op.get("model"))
        {
            return serde_json::from_value(model_val.clone()).map_err(|e| {
                format!(
                    "failed to deserialize OpportunityModelConfig from plugin.output_plugins.model: {e}"
                )
            });
        }

        // 3. Check free-standing OpportunityPluginConfig (`model` key at root)
        if let Some(model_val) = value.get("model") {
            return serde_json::from_value(model_val.clone()).map_err(|e| {
                format!("failed to deserialize OpportunityModelConfig from root 'model': {e}")
            });
        }

        // 4. Otherwise deserialize directly as a free-standing OpportunityModelConfig
        serde_json::from_value(value.clone())
            .map_err(|e| format!("failed to deserialize OpportunityModelConfig: {e}"))
    }
}

/// gets a u32 from a CSV cell by column name, when also provided a lookup table
/// giving indices by column name.
fn get_u32_from_row(
    row: &StringRecord,
    col: &String,
    lookup: &HashMap<String, usize>,
) -> Result<u32, OutputPluginError> {
    let record_index = lookup.get(col).ok_or_else(|| {
        OutputPluginError::OutputPluginFailed(format!("file is missing expected column {col}"))
    })?;
    let value = row.get(*record_index).ok_or_else(|| {
        OutputPluginError::OutputPluginFailed(format!(
            "file column {col} is missing from mapping but requested by the opportunity model"
        ))
    })?;

    let number: u32 = value.parse().map_err(|e| {
        OutputPluginError::OutputPluginFailed(format!("could not read {value} as an integer: {e}"))
    })?;
    Ok(number)
}

/// gets a u32 from a CSV cell by column name, when also provided a lookup table
/// giving indices by column name.
fn get_f64_from_row(
    row: &StringRecord,
    col: &String,
    lookup: &HashMap<String, usize>,
) -> Result<f64, OutputPluginError> {
    let record_index = lookup.get(col).ok_or_else(|| {
        OutputPluginError::OutputPluginFailed(format!("file is missing expected column {col}"))
    })?;
    let value = row.get(*record_index).ok_or_else(|| {
        OutputPluginError::OutputPluginFailed(format!(
            "file column {col} is missing from mapping but requested by the opportunity model"
        ))
    })?;

    let number: f64 = value.parse().map_err(|e| {
        OutputPluginError::OutputPluginFailed(format!(
            "could not read {value} as an f64 (floating point value): {e}"
        ))
    })?;
    Ok(number)
}

/// gets a deserializable value from a CSV cell by column name, when also provided a lookup table
/// giving indices by column name.
///
/// used when we can't rely on serde for deserialization because the size of the row is not
/// known at compile time.
///
/// hey, this fails for my expected input 'failed to deserialize column retail - invalid type: string "0", expected u32'
/// i'll skip this generic approach and do a specialized u32 get operation
fn get_from_row<T>(
    row: &StringRecord,
    col: &String,
    lookup: &HashMap<String, usize>,
) -> Result<T, OutputPluginError>
where
    T: de::DeserializeOwned,
{
    let record_index = lookup.get(col).ok_or_else(|| {
        OutputPluginError::OutputPluginFailed(format!("file is missing expected column {col}"))
    })?;
    let value = row.get(*record_index).ok_or_else(|| {
        OutputPluginError::OutputPluginFailed(format!(
            "file column {col} is missing from mapping but requested by the opportunity model"
        ))
    })?;
    use de::IntoDeserializer;
    let result: Result<T, OutputPluginError> =
        T::deserialize(value.into_deserializer()).map_err(|e: de::value::Error| {
            OutputPluginError::OutputPluginFailed(format!(
                "failed to deserialize column {col} - {e}"
            ))
        });
    result
}
