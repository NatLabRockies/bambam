use crate::model::output_plugin::opportunity::{
    opportunity_model_config::read_config_value, source::OverturePlacesMappingConfig,
    OpportunityDataset, OpportunityModelConfig,
};

use super::{
    source::lodes::lodes_ops,
    source::overture_opportunity_collection_model::{
        validate_overture_activity_keys, OvertureOpportunityCollectionModel,
    },
    study_region::StudyRegion,
};
use bambam_core::model::output_plugin::opportunity::OpportunityOrientation;
use bambam_network::GeographicExtent;
use bambam_omf::collection::{OvertureMapsCollectorConfig, OvertureSpatialFilter, ReleaseVersion};
use bamcensus::app::lodes_tiger;
use bamcensus_core::model::identifier::GeoidType;
use bamcensus_lehd::model::{
    FallbackPolicy, LodesDataset, LodesEdition, LodesJobType, WacSegment, WorkplaceSegment,
};
use geo::Geometry;
use itertools::Itertools;
use routee_compass_core::model::network::Vertex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

/// an API data source for opportunities.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum OpportunitySource {
    /// collects opportunities from a Longitudinal Employer-Household Dynamics (LODES)
    /// Workplace Area Characteristics (WAC) dataset paired with it's corresponding
    /// TIGER/Line Shapefile. the user provides a mapping from each WacSegment to a list of
    /// activity types (at least one) which it represents.
    #[serde(rename = "lodes")]
    UsCensusLehdLodes {
        activity_mapping: HashMap<WacSegment, Vec<String>>,
        study_region: StudyRegion,
        data_granularity: Option<GeoidType>,
        edition: LodesEdition,
        job_type: LodesJobType,
        segment: WorkplaceSegment,
        year: u64,
        #[serde(default)]
        fallback: FallbackPolicy,
    },
    /// collects opportunities from <https://docs.overturemaps.org/guides/places/>.
    #[serde(rename = "overture")]
    OvertureMapsPlaces {
        #[serde(default)]
        collector_config: OvertureMapsCollectorConfig,
        /// Optional geographic extent (`bbox`, inline `wkt`, or `wkt_file`).
        /// Not required when graph `vertices` are supplied during collection.
        #[serde(default)]
        extent: Option<GeographicExtent>,
        /// Optional Haversine distance tolerance in meters when filtering OMF records
        /// by proximity to graph vertices (defaults to `200.0`m).
        #[serde(default)]
        vertex_match_tolerance_meters: Option<f64>,
        places_activity_mapping: OverturePlacesMappingConfig,
        buildings_activity_mapping: Option<HashMap<String, Vec<String>>>,
        #[serde(default)]
        release_version: ReleaseVersion,
    },
}

impl OpportunitySource {
    /// generates a collection of Geometries paired with activity counts
    /// from some data source API. Configurations for a given API are
    /// provided by this [`OpportunitySource`] instance.
    ///
    /// # Arguments
    ///
    /// * `activity_types` - the types of activities expected
    ///
    /// # Returns
    ///
    /// A collection of Geometries tagged with activity rows.
    pub fn generate_dataset(
        &self,
        activity_types: &[String],
    ) -> Result<OpportunityDataset, String> {
        self.generate_dataset_with_context(activity_types, None, None)
    }

    /// Generates an [`OpportunityDataset`] using optional road network `vertices` and/or
    /// an optional `extent_override` to construct a two-stage [`OvertureSpatialFilter`]
    /// when collecting from [`OpportunitySource::OvertureMapsPlaces`].
    pub fn generate_dataset_with_context(
        &self,
        activity_types: &[String],
        vertices: Option<&[Vertex]>,
        extent_override: Option<&GeographicExtent>,
    ) -> Result<OpportunityDataset, String> {
        match self {
            OpportunitySource::OvertureMapsPlaces {
                collector_config,
                extent,
                vertex_match_tolerance_meters,
                places_activity_mapping,
                buildings_activity_mapping,
                release_version,
            } => {
                let places_mapping = places_activity_mapping.build()?;
                let effective_extent = extent_override.or(extent.as_ref());

                let spatial_filter = match (vertices, effective_extent) {
                    (Some(verts), ext_opt) if !verts.is_empty() => {
                        OvertureSpatialFilter::from_vertices(
                            verts,
                            *vertex_match_tolerance_meters,
                            ext_opt,
                        )
                        .map_err(|e| format!("Error building Overture graph spatial filter: {e}"))?
                    }
                    (_, Some(ext)) => OvertureSpatialFilter::from_extent(ext).map_err(|e| {
                        format!("Error building Overture extent spatial filter: {e}")
                    })?,
                    _ => {
                        return Err(
                            "OvertureMapsPlaces requires either graph vertices (e.g. --vertices-file) or an 'extent' (in configuration or via --extent-file)"
                                .to_string(),
                        )
                    }
                };

                // Instantiate Collection Model Object which re-structures activity mapping
                // information into a fully functional collection pipeline. This step allows
                // to reduce repetition in the configuration file by making some assumptions
                // about the filters being used.
                let colletor_model = OvertureOpportunityCollectionModel::new(
                    *collector_config,
                    release_version.clone(),
                    spatial_filter,
                    places_mapping,
                    buildings_activity_mapping.clone(),
                )
                .map_err(|e| format!("Error creating Overture OpportunityCollectionModel: {e}"))?;

                colletor_model
                    .collect(activity_types)
                    .map_err(|e| format!("Error during overturemaps collection: {e}"))
            }
            OpportunitySource::UsCensusLehdLodes {
                activity_mapping,
                study_region,
                data_granularity,
                edition,
                job_type,
                segment,
                year,
                fallback,
            } => {
                let filtered_mapping =
                    lodes_ops::filter_activity_mapping(activity_mapping, activity_types)?;
                let geoids = study_region.get_geoids()?;
                let dataset = LodesDataset::WAC {
                    edition: *edition,
                    job_type: *job_type,
                    segment: *segment,
                    year: *year,
                };
                let wac_segments = filtered_mapping.keys().cloned().collect_vec();
                lodes_ops::collect_lodes_opportunities(
                    &dataset,
                    &wac_segments,
                    &geoids,
                    data_granularity,
                    activity_types,
                    &filtered_mapping,
                    fallback,
                )
            }
        }
    }

    /// Derives the sorted, deduplicated list of activity column names defined in this
    /// [`OpportunitySource`]'s activity mapping configuration.
    pub fn default_activity_column_names(&self) -> Result<Vec<String>, String> {
        match self {
            OpportunitySource::UsCensusLehdLodes {
                activity_mapping, ..
            } => Ok(activity_mapping
                .values()
                .flatten()
                .cloned()
                .unique()
                .sorted()
                .collect_vec()),
            OpportunitySource::OvertureMapsPlaces {
                places_activity_mapping,
                buildings_activity_mapping,
                ..
            } => {
                let places_mapping = places_activity_mapping.build()?;
                validate_overture_activity_keys(
                    &places_mapping,
                    buildings_activity_mapping.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let names = places_mapping.into_keys().sorted().collect_vec();
                Ok(names)
            }
        }
    }

    /// Loads an [`OpportunitySource`] (along with optional `activity_column_names` and
    /// `table_orientation` if the file wraps it in an [`OpportunityModelConfig::ApiSource`])
    /// from a TOML or JSON file.
    pub fn from_config_path(
        path: &Path,
    ) -> Result<(Self, Option<Vec<String>>, Option<OpportunityOrientation>), String> {
        let value = read_config_value(path)?;
        Self::from_config_value(&value)
    }

    /// Extracts an [`OpportunitySource`] (plus optional `activity_column_names` and
    /// `table_orientation` if present) from a parsed [`serde_json::Value`].
    pub fn from_config_value(
        value: &serde_json::Value,
    ) -> Result<(Self, Option<Vec<String>>, Option<OpportunityOrientation>), String> {
        // 1. Check if the value can be parsed as an OpportunityModelConfig (e.g. ApiSource or Combined)
        if let Ok(model_config) = OpportunityModelConfig::try_from(value) {
            return Self::from_model_config(model_config);
        }

        // 2. Check if there is an `opportunity_source` key at the root, under `model`, or under `plugin.output_plugins.model`
        let container_opt = if value.get("opportunity_source").is_some() {
            Some(value)
        } else if let Some(m) = value
            .get("model")
            .filter(|m| m.get("opportunity_source").is_some())
        {
            Some(m)
        } else {
            value
                .get("plugin")
                .and_then(|p| p.get("output_plugins"))
                .and_then(|op| op.get("model"))
                .filter(|m| m.get("opportunity_source").is_some())
        };

        if let Some(container) = container_opt {
            let source_val = container.get("opportunity_source").unwrap();
            let source: OpportunitySource =
                serde_json::from_value(source_val.clone()).map_err(|e| {
                    format!(
                        "failed to deserialize OpportunitySource from 'opportunity_source': {e}"
                    )
                })?;
            let activity_cols: Option<Vec<String>> = container
                .get("activity_column_names")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|e| format!("failed to parse 'activity_column_names': {e}"))?;
            let orientation: Option<OpportunityOrientation> = container
                .get("table_orientation")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|e| format!("failed to parse 'table_orientation': {e}"))?;
            return Ok((source, activity_cols, orientation));
        }

        // 3. Otherwise deserialize directly as a free-standing OpportunitySource
        let source: OpportunitySource = serde_json::from_value(value.clone())
            .map_err(|e| format!("failed to deserialize OpportunitySource: {e}"))?;
        Ok((source, None, None))
    }

    fn from_model_config(
        model_config: OpportunityModelConfig,
    ) -> Result<(Self, Option<Vec<String>>, Option<OpportunityOrientation>), String> {
        match model_config {
            OpportunityModelConfig::ApiSource {
                opportunity_source,
                activity_column_names,
                table_orientation,
                ..
            } => Ok((
                opportunity_source,
                Some(activity_column_names),
                Some(table_orientation),
            )),
            OpportunityModelConfig::Combined { mut models } => {
                if models.len() == 1 {
                    Self::from_model_config(*models.remove(0))
                } else {
                    Err(format!(
                        "expected a single OpportunitySource or ApiSource, but found a Combined model with {} inner models",
                        models.len()
                    ))
                }
            }
            OpportunityModelConfig::FileSource { .. } => Err(
                "expected an OpportunitySource or ApiSource config, but found a FileSource config"
                    .to_string(),
            ),
        }
    }
}

impl TryFrom<&Path> for OpportunitySource {
    type Error = String;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let (source, _, _) = Self::from_config_path(path)?;
        Ok(source)
    }
}

impl TryFrom<&serde_json::Value> for OpportunitySource {
    type Error = String;

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        let (source, _, _) = Self::from_config_value(value)?;
        Ok(source)
    }
}
