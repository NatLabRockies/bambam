use std::collections::HashMap;

use bamcensus_acs::model::AcsType;
use bamcensus_core::model::identifier::GeoidType;
use bamcensus_lehd::model::{
    FallbackPolicy, LodesEdition, LodesJobType, WacSegment, WorkplaceSegment,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::CongestionError;

/// Data source for Census population and employment density polygons.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CensusSourceConfig {
    /// Collect Census TIGER/Line polygons tagged with population (from US Census ACS) and
    /// employment (from LEHD LODES WAC) via `bamcensus`.
    ///
    /// Requires the user's Census API key in `CENSUS_API_KEY` (or `CENSUS_API_TOKEN`) in the environment.
    #[serde(alias = "acs_lodes")]
    Bamcensus {
        /// ACS population configuration (mirrors `PopulationSourceConfig::UsCensusAcs`).
        #[serde(default)]
        population_source: BamcensusPopulationConfig,
        /// LEHD LODES employment configuration (mirrors `OpportunitySource::UsCensusLehdLodes`).
        #[serde(default)]
        employment_source: BamcensusLodesConfig,
        /// Optional path to save the downloaded Census polygons (`geoid,population,jobs,geometry`)
        /// as a CSV or `.csv.gz` fixture (can also be set via `--save-bamcensus-csv`).
        #[serde(default)]
        save_csv_path: Option<String>,
    },
    /// Read a prepared Census shapefile containing `pop-den` and `emp-den` (per $\text{km}^2$) or
    /// `population` and `employment` counts.
    Shapefile {
        path: String,
        #[serde(default = "default_pop_den_field")]
        pop_density_field: String,
        #[serde(default = "default_emp_den_field")]
        emp_density_field: String,
    },
    /// Read Census polygons from a WKT CSV (such as `tiger-lines/tracts.csv` with `GEOID` and `geometry`)
    /// joined with population and/or employment CSV tables by GEOID, or inline density columns.
    CsvPolygons {
        polygons_csv_path: String,
        #[serde(default = "default_geoid_col")]
        geoid_column: String,
        #[serde(default = "default_geom_col")]
        wkt_column: String,
        /// Optional inline column in `polygons_csv_path` for population count.
        #[serde(default)]
        population_column: Option<String>,
        /// Optional inline column in `polygons_csv_path` for employment count.
        #[serde(default)]
        employment_column: Option<String>,
        /// Optional external CSV mapping `geoid -> population` (e.g. ACS table).
        #[serde(default)]
        population_csv_path: Option<String>,
        #[serde(default)]
        population_csv_geoid_column: Option<String>,
        #[serde(default)]
        population_csv_value_column: Option<String>,
        /// Optional external CSV mapping `geoid -> employment` (e.g. LODES WAC table with `C000` or `jobs`).
        #[serde(default)]
        employment_csv_path: Option<String>,
        #[serde(default)]
        employment_csv_geoid_column: Option<String>,
        #[serde(default)]
        employment_csv_value_column: Option<String>,
    },
}

/// Configuration for downloading US Census ACS population counts paired with TIGER/Line polygons
/// via `bamcensus` (configured similarly to `PopulationSourceConfig` in `bambam.toml`).
///
/// The Census API key is read from the `CENSUS_API_KEY` (or `CENSUS_API_TOKEN`) environment variable,
/// or can be supplied explicitly via `api_token`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BamcensusPopulationConfig {
    /// Optional `type = "acs"` tag for structural compatibility with `[plugin.input_plugins.population_source]`.
    #[serde(default)]
    pub r#type: Option<String>,
    /// ACS survey type (`"five_year"` or `"one_year"`).
    #[serde(default = "default_acs_type")]
    #[schemars(with = "String")]
    pub acs_type: AcsType,
    /// ACS survey year (e.g. `2022`).
    #[serde(default = "default_acs_year")]
    pub acs_year: u64,
    /// Geographic resolution for ACS & TIGER polygons (e.g. `"census_tract"`, `"block_group"`).
    #[serde(default = "default_acs_resolution")]
    #[schemars(with = "Option<String>")]
    pub acs_resolution: Option<GeoidType>,
    /// ACS table variables to sum as total population (defaults to `["B01001_001E"]`).
    #[serde(default)]
    pub acs_categories: Option<Vec<String>>,
    /// Optional explicit Census API key override; when `None`, reads `CENSUS_API_KEY` or `CENSUS_API_TOKEN` from the environment.
    #[serde(default)]
    pub api_token: Option<String>,
}

impl Default for BamcensusPopulationConfig {
    fn default() -> Self {
        Self {
            r#type: Some("acs".to_string()),
            acs_type: default_acs_type(),
            acs_year: default_acs_year(),
            acs_resolution: default_acs_resolution(),
            acs_categories: None,
            api_token: None,
        }
    }
}

impl BamcensusPopulationConfig {
    /// Resolves the Census API key from `self.api_token`, `CENSUS_API_KEY`, or `CENSUS_API_TOKEN`.
    pub fn resolve_api_token(&self) -> Result<String, CongestionError> {
        if let Some(ref token) = self.api_token {
            if !token.trim().is_empty() {
                return Ok(token.trim().to_string());
            }
        }
        std::env::var("CENSUS_API_KEY")
            .or_else(|_| std::env::var("CENSUS_API_TOKEN"))
            .map_err(|_| {
                CongestionError::SpatialFeature(
                    "ACS download requires a Census API key; please set the 'CENSUS_API_KEY' (or 'CENSUS_API_TOKEN') environment variable".to_string(),
                )
            })
    }
}

/// Study region specification for `bamcensus` LODES / ACS queries (matches `StudyRegion` in `bambam`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CensusStudyRegion {
    /// Query all US states.
    National,
    /// Query specific state or county FIPS GEOIDs (e.g. `{ type = "census", geoids = ["08013"] }`).
    Census { geoids: Vec<String> },
}

/// Configuration for downloading LEHD LODES Workplace Area Characteristics (WAC) employment counts
/// via `bamcensus` (configured similarly to `OpportunitySource::UsCensusLehdLodes` in `bambam.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BamcensusLodesConfig {
    /// Optional `type = "lodes"` tag for structural compatibility with `[plugin.output_plugins.model.models.opportunity_source]`.
    #[serde(default)]
    pub r#type: Option<String>,
    /// Optional study region (`{ type = "census", geoids = ["08013", ...] }` or `{ type = "national" }`).
    /// If omitted, states intersecting the Open Network's bounding box are automatically queried.
    #[serde(default)]
    pub study_region: Option<CensusStudyRegion>,
    /// Geographic granularity to aggregate LODES employment counts to (defaults to `"census_tract"`).
    #[serde(default = "default_acs_resolution")]
    #[schemars(with = "Option<String>")]
    pub data_granularity: Option<GeoidType>,
    /// LODES edition (defaults to `"LODES8"`).
    #[serde(default)]
    #[schemars(with = "String")]
    pub edition: LodesEdition,
    /// LODES job type (defaults to `"JT00"` for All Jobs).
    #[serde(default)]
    #[schemars(with = "String")]
    pub job_type: LodesJobType,
    /// LODES workforce segment (defaults to `"S000"` for Total Jobs).
    #[serde(default)]
    #[schemars(with = "String")]
    pub segment: WorkplaceSegment,
    /// LODES dataset year (defaults to `2022`).
    #[serde(default = "default_acs_year")]
    pub year: u64,
    /// Optional explicit WAC segments to sum for total employment (defaults to `[C000]`).
    #[serde(default)]
    #[schemars(with = "Option<Vec<String>>")]
    pub wac_segments: Option<Vec<WacSegment>>,
    /// Optional `activity_mapping` table (as in `bambam.toml`); if provided and `wac_segments` is `None`,
    /// all WAC segments mapped to `"jobs"` (or all keys if `"jobs"` is absent) are summed.
    #[serde(default)]
    #[schemars(with = "Option<HashMap<String, Vec<String>>>")]
    pub activity_mapping: Option<HashMap<WacSegment, Vec<String>>>,

    #[serde(default)]
    pub fallback: FallbackPolicyConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct FallbackPolicyConfig(pub FallbackPolicy);

impl JsonSchema for FallbackPolicyConfig {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        FallbackPolicySchema::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        FallbackPolicySchema::json_schema(generator)
    }
}

/// Schema mirror for [`FallbackPolicy`] (`bamcensus_lehd::model::FallbackPolicy`).
#[derive(JsonSchema)]
#[schemars(rename = "FallbackPolicyConfig")]
#[allow(dead_code)]
enum FallbackPolicySchema {
    /// Do not retry; fail immediately if the requested state/year is unavailable
    FailAlways,
    /// If the requested year fails, try N years earlier
    TryPreviousYears(u64),
    /// If the requested year fails, try N years later
    TryNextYears(u64),
    /// If the requested year fails, try up to N years earlier or M years later
    BiDirectional { past: u64, future: u64 },
}

impl Default for BamcensusLodesConfig {
    fn default() -> Self {
        Self {
            r#type: Some("lodes".to_string()),
            study_region: None,
            data_granularity: default_acs_resolution(),
            edition: LodesEdition::default(),
            job_type: LodesJobType::default(),
            segment: WorkplaceSegment::default(),
            year: default_acs_year(),
            wac_segments: None,
            activity_mapping: None,
            fallback: FallbackPolicyConfig::default(),
        }
    }
}

impl BamcensusLodesConfig {
    /// Resolves the list of `WacSegment`s whose values should be summed to compute total employment.
    ///
    /// - If `wac_segments` is explicitly provided, uses those segments.
    /// - Otherwise, if `activity_mapping` is provided (like in `bambam.toml`), collects all segments
    ///   mapped to `"jobs"` (or all keys if `"jobs"` is not present).
    /// - Otherwise defaults to `vec![WacSegment::C000]` (total jobs).
    pub fn resolved_wac_segments(&self) -> Vec<WacSegment> {
        if let Some(ref segs) = self.wac_segments {
            if !segs.is_empty() {
                return segs.clone();
            }
        }
        if let Some(ref mapping) = self.activity_mapping {
            let job_segs: Vec<WacSegment> = mapping
                .iter()
                .filter_map(|(seg, acts)| {
                    if acts.iter().any(|a| a.eq_ignore_ascii_case("jobs")) {
                        Some(*seg)
                    } else {
                        None
                    }
                })
                .collect();
            if !job_segs.is_empty() {
                return job_segs;
            }
            if !mapping.is_empty() {
                return mapping.keys().copied().collect();
            }
        }
        vec![WacSegment::C000]
    }
}

fn default_pop_den_field() -> String {
    "pop-den".to_string()
}

fn default_emp_den_field() -> String {
    "emp-den".to_string()
}

fn default_geoid_col() -> String {
    "GEOID".to_string()
}

fn default_geom_col() -> String {
    "geometry".to_string()
}

fn default_acs_type() -> AcsType {
    AcsType::FiveYear
}

fn default_acs_year() -> u64 {
    2022
}

fn default_acs_resolution() -> Option<GeoidType> {
    Some(GeoidType::CensusTract)
}
