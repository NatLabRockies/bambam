use std::path::Path;

use clap::{Args, Parser, Subcommand};
use config::{Config, File};
use routee_compass_core::model::unit::SpeedUnit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::CongestionError;

/// CLI entry point for `bambam-congestion`.
#[derive(Debug, Parser)]
#[command(
    name = "bambam-congestion",
    author,
    version,
    about = "Congestion calibration and operating speed imputation for BAMBAM and RouteE Compass road networks"
)]
#[command(propagate_version = true)]
pub struct CongestionApp {
    #[command(subcommand)]
    pub command: CongestionCommand,
}

#[derive(Debug, Clone, Subcommand, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CongestionCommand {
    /// Conflate an Open Network (OSM/OMF) with a TomTom MultiNet-R Compass dataset,
    /// compute 0.5-mile spatial features, and export calibration training tables (Parquet/CSV).
    Calibrate(CalibrateCliArgs),

    /// Impute operating speeds across all 6 time-of-day periods (`wd-am`, `wd-af`, `wd-pm`,
    /// `we-am`, `we-af`, `we-pm`) for a BAMBAM / RouteE Compass road network using calibrated XGBoost models.
    Impute(ImputeCliArgs),
}

/// CLI arguments for `bambam-congestion calibrate`.
///
/// Complex configurations can be passed via `--config <path.toml|.json>`, with optional
/// CLI flag overrides.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct CalibrateCliArgs {
    /// Path to a TOML or JSON calibration configuration file (`CalibrationConfig`).
    #[arg(short, long)]
    pub config: Option<String>,

    /// Override: path to the Open Network (OSM/OMF) RouteE Compass directory.
    #[arg(long)]
    pub open_network_dir: Option<String>,

    /// Override: travel mode subdirectory in the Open Network (defaults to `"drive"`).
    #[arg(long)]
    pub mode: Option<String>,

    /// Override: path to the TomTom MultiNet-R RouteE Compass directory.
    #[arg(long)]
    pub tomtom_network_dir: Option<String>,

    /// Override: directory containing `tomtom-natl-{wd,we}-{am,af,pm}.txt.gz` speed files.
    #[arg(long)]
    pub tomtom_speeds_dir: Option<String>,

    /// Override: output directory for calibration Parquet/CSV tables.
    #[arg(short, long)]
    pub output_dir: Option<String>,

    /// Optional `.osm.pbf` path for extracting MEP land-use opportunities and traffic controls.
    #[arg(long)]
    pub osm_pbf_path: Option<String>,
}

impl CalibrateCliArgs {
    pub fn resolve_config(&self) -> Result<CalibrationConfig, CongestionError> {
        let mut cfg = if let Some(ref config_path) = self.config {
            CalibrationConfig::from_file(Path::new(config_path))?
        } else {
            let open_dir = self.open_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --open-network-dir".to_string(),
                )
            })?;
            let tomtom_dir = self.tomtom_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --tomtom-network-dir".to_string(),
                )
            })?;
            let speeds_dir = self.tomtom_speeds_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --tomtom-speeds-dir".to_string(),
                )
            })?;
            let output_dir = self.output_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --output-dir".to_string(),
                )
            })?;
            CalibrationConfig {
                open_network: OpenNetworkConfig {
                    network_dir: open_dir,
                    mode: self.mode.clone().unwrap_or_else(|| "drive".to_string()),
                    ..Default::default()
                },
                tomtom_network: TomTomNetworkConfig {
                    network_dir: tomtom_dir,
                    period_speed_files: PeriodSpeedFilesConfig::from_directory(
                        Path::new(&speeds_dir),
                        "tomtom-natl",
                    ),
                    ..Default::default()
                },
                conflation: ConflationConfig::default(),
                spatial_features: SpatialFeaturesConfig::default(),
                output: CalibrationOutputConfig {
                    output_dir,
                    ..Default::default()
                },
            }
        };

        if let Some(ref open_dir) = self.open_network_dir {
            cfg.open_network.network_dir = open_dir.clone();
        }
        if let Some(ref mode) = self.mode {
            cfg.open_network.mode = mode.clone();
        }
        if let Some(ref tomtom_dir) = self.tomtom_network_dir {
            cfg.tomtom_network.network_dir = tomtom_dir.clone();
        }
        if let Some(ref speeds_dir) = self.tomtom_speeds_dir {
            cfg.tomtom_network.period_speed_files =
                PeriodSpeedFilesConfig::from_directory(Path::new(speeds_dir), "tomtom-natl");
        }
        if let Some(ref out_dir) = self.output_dir {
            cfg.output.output_dir = out_dir.clone();
        }
        if let Some(ref pbf_path) = self.osm_pbf_path {
            cfg.spatial_features.opportunities_source =
                Some(OpportunitiesSourceConfig::OsmPbf {
                    pbf_path: pbf_path.clone(),
                });
        }

        Ok(cfg)
    }
}

/// CLI arguments for `bambam-congestion impute`.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct ImputeCliArgs {
    /// Path to a TOML or JSON imputation configuration file (`ImputationConfig`).
    #[arg(short, long)]
    pub config: Option<String>,

    /// Override: path to the Open Network (OSM/OMF) RouteE Compass directory.
    #[arg(long)]
    pub open_network_dir: Option<String>,

    /// Override: travel mode subdirectory in the Open Network (defaults to `"drive"`).
    #[arg(long)]
    pub mode: Option<String>,

    /// Override: path to the calibrated `with_speedlimit` XGBoost `.json` model file.
    #[arg(long)]
    pub model_with_speedlimit: Option<String>,

    /// Override: path to the calibrated `without_speedlimit` XGBoost `.json` model file.
    #[arg(long)]
    pub model_without_speedlimit: Option<String>,

    /// Override: optional output directory (defaults to `<open_network_dir>/<mode>`).
    #[arg(short, long)]
    pub output_dir: Option<String>,

    /// Optional `.osm.pbf` path for extracting MEP land-use opportunities and traffic controls.
    #[arg(long)]
    pub osm_pbf_path: Option<String>,
}

impl ImputeCliArgs {
    pub fn resolve_config(&self) -> Result<ImputationConfig, CongestionError> {
        let mut cfg = if let Some(ref config_path) = self.config {
            ImputationConfig::from_file(Path::new(config_path))?
        } else {
            let open_dir = self.open_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --open-network-dir".to_string(),
                )
            })?;
            let model_with_sl = self.model_with_speedlimit.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --model-with-speedlimit".to_string(),
                )
            })?;
            let model_without_sl = self.model_without_speedlimit.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --model-without-speedlimit".to_string(),
                )
            })?;
            ImputationConfig {
                open_network: OpenNetworkConfig {
                    network_dir: open_dir,
                    mode: self.mode.clone().unwrap_or_else(|| "drive".to_string()),
                    ..Default::default()
                },
                spatial_features: SpatialFeaturesConfig::default(),
                models: ImputationModelConfig {
                    model_with_speedlimit_path: model_with_sl,
                    model_without_speedlimit_path: model_without_sl,
                },
                output_dir: self.output_dir.clone(),
                write_parquet_summary: true,
                write_csv_summary: true,
            }
        };

        if let Some(ref open_dir) = self.open_network_dir {
            cfg.open_network.network_dir = open_dir.clone();
        }
        if let Some(ref mode) = self.mode {
            cfg.open_network.mode = mode.clone();
        }
        if let Some(ref m_with) = self.model_with_speedlimit {
            cfg.models.model_with_speedlimit_path = m_with.clone();
        }
        if let Some(ref m_without) = self.model_without_speedlimit {
            cfg.models.model_without_speedlimit_path = m_without.clone();
        }
        if let Some(ref out_dir) = self.output_dir {
            cfg.output_dir = Some(out_dir.clone());
        }
        if let Some(ref pbf_path) = self.osm_pbf_path {
            cfg.spatial_features.opportunities_source =
                Some(OpportunitiesSourceConfig::OsmPbf {
                    pbf_path: pbf_path.clone(),
                });
        }

        Ok(cfg)
    }
}

/// Top-level configuration for the congestion calibration workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationConfig {
    pub open_network: OpenNetworkConfig,
    pub tomtom_network: TomTomNetworkConfig,
    #[serde(default)]
    pub conflation: ConflationConfig,
    #[serde(default)]
    pub spatial_features: SpatialFeaturesConfig,
    pub output: CalibrationOutputConfig,
}

impl CalibrationConfig {
    pub fn from_file(filepath: &Path) -> Result<Self, CongestionError> {
        let cfg = Config::builder()
            .add_source(File::from(filepath))
            .build()
            .map_err(|e| {
                CongestionError::Configuration(format!(
                    "failed to read config '{}': {e}",
                    filepath.display()
                ))
            })?;
        cfg.try_deserialize().map_err(|e| {
            CongestionError::Configuration(format!(
                "failed to deserialize CalibrationConfig from '{}': {e}",
                filepath.display()
            ))
        })
    }
}

/// Top-level configuration for the operating speed imputation workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImputationConfig {
    pub open_network: OpenNetworkConfig,
    #[serde(default)]
    pub spatial_features: SpatialFeaturesConfig,
    pub models: ImputationModelConfig,
    /// Optional output directory; defaults to `<open_network.network_dir>/<open_network.mode>`.
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default = "default_true")]
    pub write_parquet_summary: bool,
    #[serde(default = "default_true")]
    pub write_csv_summary: bool,
}

impl ImputationConfig {
    pub fn from_file(filepath: &Path) -> Result<Self, CongestionError> {
        let cfg = Config::builder()
            .add_source(File::from(filepath))
            .build()
            .map_err(|e| {
                CongestionError::Configuration(format!(
                    "failed to read config '{}': {e}",
                    filepath.display()
                ))
            })?;
        cfg.try_deserialize().map_err(|e| {
            CongestionError::Configuration(format!(
                "failed to deserialize ImputationConfig from '{}': {e}",
                filepath.display()
            ))
        })
    }
}

/// Configuration for loading an Open Network (OSM or Overture Maps) in RouteE Compass format.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OpenNetworkConfig {
    /// Root directory of the imported network (or flat Compass directory).
    pub network_dir: String,
    /// Mode subdirectory (e.g. `"drive"`). If `<network_dir>/<mode>` does not exist,
    /// `<network_dir>` itself is inspected for flat Compass files.
    #[serde(default = "default_drive_mode")]
    pub mode: String,
    /// Speed unit for legacy `edges-posted-speed-enumerated.txt.gz` files when
    /// `edges-posted-speeds-mph-enumerated.txt.gz` is not present (defaults to `KPH` for legacy osmnx exports).
    #[serde(default = "default_kph_unit")]
    #[schemars(with = "String")]
    pub legacy_posted_speed_unit: SpeedUnit,
    /// Minimum floor (mph) applied to median-imputed speed limits (`sl-med-imp`, defaults to `15.0`).
    #[serde(default = "default_min_speed_limit_mph")]
    pub min_imputed_speed_limit_mph: f64,
}

impl Default for OpenNetworkConfig {
    fn default() -> Self {
        Self {
            network_dir: String::new(),
            mode: default_drive_mode(),
            legacy_posted_speed_unit: default_kph_unit(),
            min_imputed_speed_limit_mph: default_min_speed_limit_mph(),
        }
    }
}

/// Configuration for the reference TomTom MultiNet-R Compass dataset.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TomTomNetworkConfig {
    /// Directory containing `edges-geometries-enumerated.txt.gz` (and optionally
    /// `edges-speed-profile-id-mapping-enumerated.txt.gz` and `speed-profile.csv.gz`).
    pub network_dir: String,
    /// Speed unit of the values in `period_speed_files` (defaults to `SpeedUnit::KPH`).
    #[serde(default = "default_kph_unit")]
    #[schemars(with = "String")]
    pub speed_unit: SpeedUnit,
    /// When `true`, only TomTom edges with time-varying speed profiles (i.e. not flat daily fallback)
    /// are used as ground-truth calibration targets.
    #[serde(default = "default_true")]
    pub require_time_varying_trace: bool,
    /// Enumerated speed files for the 6 time periods.
    pub period_speed_files: PeriodSpeedFilesConfig,
}

impl Default for TomTomNetworkConfig {
    fn default() -> Self {
        Self {
            network_dir: String::new(),
            speed_unit: default_kph_unit(),
            require_time_varying_trace: true,
            period_speed_files: PeriodSpeedFilesConfig::default(),
        }
    }
}

/// Paths to the 6 enumerated speed files (`wd-am`, `wd-af`, `wd-pm`, `we-am`, `we-af`, `we-pm`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PeriodSpeedFilesConfig {
    pub wd_am: String,
    pub wd_af: String,
    pub wd_pm: String,
    pub we_am: String,
    pub we_af: String,
    pub we_pm: String,
}

impl PeriodSpeedFilesConfig {
    /// Constructs file paths following `<dir>/<prefix>-{wd,we}-{am,af,pm}.txt.gz`.
    pub fn from_directory(dir: &Path, prefix: &str) -> Self {
        Self {
            wd_am: dir
                .join(format!("{prefix}-wd-am.txt.gz"))
                .to_string_lossy()
                .into_owned(),
            wd_af: dir
                .join(format!("{prefix}-wd-af.txt.gz"))
                .to_string_lossy()
                .into_owned(),
            wd_pm: dir
                .join(format!("{prefix}-wd-pm.txt.gz"))
                .to_string_lossy()
                .into_owned(),
            we_am: dir
                .join(format!("{prefix}-we-am.txt.gz"))
                .to_string_lossy()
                .into_owned(),
            we_af: dir
                .join(format!("{prefix}-we-af.txt.gz"))
                .to_string_lossy()
                .into_owned(),
            we_pm: dir
                .join(format!("{prefix}-we-pm.txt.gz"))
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// Parameters controlling Open Network <-> TomTom spatial conflation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConflationConfig {
    /// Corridor buffer distance in meters around each open-network edge (defaults to `5.0`).
    #[serde(default = "default_buffer_distance_meters")]
    pub buffer_distance_meters: f64,
    /// Maximum circular mean heading difference in degrees (`[0, 180]`, defaults to `45.0`).
    #[serde(default = "default_max_heading_diff_degrees")]
    pub max_heading_diff_degrees: f64,
    /// Minimum overlapping corridor length in meters required to accept a TomTom match (defaults to `1.0`).
    #[serde(default = "default_min_overlap_meters")]
    pub min_overlap_meters: f64,
    /// Whether to cap observed TomTom speeds above `posted_speed + 10 mph` (or `max_class_speed + 5 mph`).
    #[serde(default = "default_true")]
    pub cap_speed_outliers: bool,
    /// Maximum allowed mph above a link's posted speed limit (defaults to `10.0`).
    #[serde(default = "default_cap_above_posted_mph")]
    pub speed_cap_above_posted_mph: f64,
    /// Maximum allowed mph above a highway class's maximum speed limit when link speed limit is missing (defaults to `5.0`).
    #[serde(default = "default_cap_above_class_max_mph")]
    pub speed_cap_above_class_max_mph: f64,
}

impl Default for ConflationConfig {
    fn default() -> Self {
        Self {
            buffer_distance_meters: default_buffer_distance_meters(),
            max_heading_diff_degrees: default_max_heading_diff_degrees(),
            min_overlap_meters: default_min_overlap_meters(),
            cap_speed_outliers: true,
            speed_cap_above_posted_mph: default_cap_above_posted_mph(),
            speed_cap_above_class_max_mph: default_cap_above_class_max_mph(),
        }
    }
}

/// Coordinate projection mode for 0.5-mile buffer distance and area calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    /// True geodesic meters (local tangent plane centered on each edge, area = $\pi R^2 + 2RL$).
    #[default]
    Geodesic,
    /// Legacy Web Mercator (`EPSG:3857`) planar units for exact parity with legacy Python scripts.
    LegacyEpsg3857,
}

/// Configuration for the 0.5-mile spatial feature engine (POIs, traffic controls, and Census densities).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpatialFeaturesConfig {
    /// Buffer radius in miles around each edge (defaults to `0.5` miles = `804.672` meters).
    #[serde(default = "default_density_radius_miles")]
    pub density_radius_miles: f64,
    /// Coordinate projection mode (`geodesic` or `legacy_epsg3857`).
    #[serde(default)]
    pub projection_mode: ProjectionMode,
    /// Optional source for MEP land-use opportunity points.
    #[serde(default)]
    pub opportunities_source: Option<OpportunitiesSourceConfig>,
    /// Optional override source for traffic controls (`traffic_signals`, `stop`, `crossing`).
    /// When `None`, traffic controls are automatically loaded from `<open_network>/vertices-complete.csv.gz`
    /// (or extracted from `opportunities_source` if it is an `.osm.pbf`).
    #[serde(default)]
    pub traffic_controls_source: Option<TrafficControlsSourceConfig>,
    /// Optional source for Census population and employment density polygons.
    #[serde(default)]
    pub census_source: Option<CensusSourceConfig>,
}

impl Default for SpatialFeaturesConfig {
    fn default() -> Self {
        Self {
            density_radius_miles: default_density_radius_miles(),
            projection_mode: ProjectionMode::Geodesic,
            opportunities_source: None,
            traffic_controls_source: None,
            census_source: None,
        }
    }
}

/// Data source for MEP land-use opportunities (`Opp Density`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpportunitiesSourceConfig {
    /// Extract MEP land-use opportunities (and traffic controls) directly from a local `.osm.pbf` file.
    OsmPbf { pbf_path: String },
    /// Read point opportunities from a CSV or `.csv.gz` file containing `x`/`y` (lon/lat) or WKT `geometry`
    /// and an optional count column.
    CsvPoints {
        path: String,
        #[serde(default = "default_lon_col")]
        x_column: String,
        #[serde(default = "default_lat_col")]
        y_column: String,
        #[serde(default)]
        wkt_column: Option<String>,
        #[serde(default)]
        weight_column: Option<String>,
    },
    /// Read vertex-oriented opportunity counts (e.g. BAMBAM `opportunities.csv`) joined to `vertices-compass.csv.gz`.
    VertexOpportunitiesCsv {
        opportunities_csv_path: String,
        /// Optional list of columns to sum as MEP opportunities; if `None`, sums all numeric columns.
        #[serde(default)]
        columns: Option<Vec<String>>,
    },
}

/// Data source for traffic control points (`Traffic Signal/Crossing/Traffic Stop Density`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrafficControlsSourceConfig {
    /// Use `<open_network>/vertices-complete.csv.gz` (`traffic_signals`, `stop`, `crossing`).
    VerticesComplete {
        #[serde(default)]
        path: Option<String>,
    },
    /// Extract traffic control nodes directly from a local `.osm.pbf` file.
    OsmPbf { pbf_path: String },
    /// Read traffic control points from a CSV file with `x`, `y` (lon/lat) and optional `weight` column.
    CsvPoints {
        path: String,
        #[serde(default = "default_lon_col")]
        x_column: String,
        #[serde(default = "default_lat_col")]
        y_column: String,
    },
}

/// Data source for Census population and employment density polygons.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CensusSourceConfig {
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

/// Output configuration for `bambam-congestion calibrate`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationOutputConfig {
    pub output_dir: String,
    #[serde(default = "default_true")]
    pub write_parquet: bool,
    #[serde(default = "default_true")]
    pub write_csv: bool,
}

impl Default for CalibrationOutputConfig {
    fn default() -> Self {
        Self {
            output_dir: String::new(),
            write_parquet: true,
            write_csv: true,
        }
    }
}

/// Model artifact paths for `bambam-congestion impute`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImputationModelConfig {
    /// Path to the calibrated `with_speedlimit` XGBoost JSON model (`model.save_model("...json")`).
    pub model_with_speedlimit_path: String,
    /// Path to the calibrated `without_speedlimit` XGBoost JSON model (`model.save_model("...json")`).
    pub model_without_speedlimit_path: String,
}

fn default_drive_mode() -> String {
    "drive".to_string()
}

fn default_kph_unit() -> SpeedUnit {
    SpeedUnit::KPH
}

fn default_min_speed_limit_mph() -> f64 {
    15.0
}

fn default_buffer_distance_meters() -> f64 {
    5.0
}

fn default_max_heading_diff_degrees() -> f64 {
    45.0
}

fn default_min_overlap_meters() -> f64 {
    1.0
}

fn default_cap_above_posted_mph() -> f64 {
    10.0
}

fn default_cap_above_class_max_mph() -> f64 {
    5.0
}

fn default_density_radius_miles() -> f64 {
    0.5
}

fn default_true() -> bool {
    true
}

fn default_lon_col() -> String {
    "x".to_string()
}

fn default_lat_col() -> String {
    "y".to_string()
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
