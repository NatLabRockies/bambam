use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use bamcensus_acs::model::AcsType;
use bamcensus_core::model::identifier::GeoidType;
use bamcensus_lehd::model::{
    FallbackPolicy, LodesEdition, LodesJobType, WacSegment, WorkplaceSegment,
};
use clap::{Args, Parser, Subcommand};
use config::{Config, File};
use routee_compass_core::model::unit::SpeedUnit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::CongestionError, taxonomy::TimeBinSpec};

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
    /// Conflate an Open Network (OSM/OMF) with a Reference Road Network in RouteE Compass format,
    /// compute 0.5-mile spatial features, and export calibration training tables (Parquet/CSV).
    Calibrate(CalibrateCliArgs),

    /// Impute operating speeds across all configured `time_bins` for a BAMBAM / RouteE Compass
    /// road network using calibrated XGBoost models.
    Impute(ImputeCliArgs),

    /// Print the configured time-bin names and indicator flags (from `--config` or the default 6 BAMBAM time bins).
    ListTimeBins(ListTimeBinsCliArgs),

    /// Download Census TIGER/Line polygons tagged with ACS population and LEHD LODES employment
    /// via `bamcensus` for an Open Network's bounding box and save them to a CSV (or `.csv.gz`) file.
    ExportCensus(ExportCensusCliArgs),
}

/// CLI arguments for `bambam-congestion export-census`.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct ExportCensusCliArgs {
    /// Optional path to a `congestion.toml` configuration file.
    #[arg(short, long)]
    pub config: Option<String>,

    /// Optional override: path to the Open Network directory (used to determine the study bounding box).
    #[arg(long)]
    pub open_network_dir: Option<String>,

    /// Output CSV (or `.csv.gz`) file path to write the `bamcensus` polygons (`geoid,population,jobs,geometry`).
    #[arg(short, long)]
    pub output_csv: String,
}

/// CLI arguments for `bambam-congestion list-time-bins`.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct ListTimeBinsCliArgs {
    /// Optional path to a `congestion.toml` configuration file. If omitted, prints the default 6 BAMBAM time bins.
    #[arg(short, long)]
    pub config: Option<String>,
}

/// CLI arguments for `bambam-congestion calibrate`.
///
/// Accepts a unified `congestion.toml` (`CongestionPipelineConfig`) or standalone `CalibrationConfig`
/// via `--config <path.toml|.json>`, with optional CLI flag overrides.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct CalibrateCliArgs {
    /// Path to a TOML or JSON configuration file (`congestion.toml`).
    #[arg(short, long)]
    pub config: Option<String>,

    /// Override: path to the Open Network (OSM/OMF) RouteE Compass directory.
    #[arg(long)]
    pub open_network_dir: Option<String>,

    /// Override: travel mode subdirectory in the Open Network (defaults to `"drive"`).
    #[arg(long)]
    pub mode: Option<String>,

    /// Override: path to the Reference Road Network RouteE Compass directory.
    #[arg(long, visible_alias = "tomtom-network-dir")]
    pub reference_network_dir: Option<String>,

    /// Override: directory containing enumerated reference speed files per time bin.
    #[arg(long, visible_alias = "tomtom-speeds-dir")]
    pub reference_speeds_dir: Option<String>,

    /// Override: filename prefix for reference speed files in `--reference-speeds-dir`
    /// (`<prefix>-<bin>.txt.gz`).
    #[arg(long)]
    pub reference_file_prefix: Option<String>,

    /// Override: output directory for calibration Parquet/CSV tables.
    #[arg(short, long)]
    pub output_dir: Option<String>,

    /// Optional `.osm.pbf` path for extracting MEP land-use opportunities and traffic controls.
    #[arg(long)]
    pub osm_pbf_path: Option<String>,

    /// Optional CSV (or `.csv.gz`) path to save the downloaded `bamcensus` dataset
    /// (`geoid,population,jobs,geometry`) when using `CensusSourceConfig::Bamcensus`.
    #[arg(long)]
    pub save_bamcensus_csv: Option<String>,
}

impl CalibrateCliArgs {
    pub fn resolve_config(&self) -> Result<CalibrationConfig, CongestionError> {
        let mut cfg = if let Some(ref config_path) = self.config {
            let path = Path::new(config_path);
            if let Ok(pipeline) = CongestionPipelineConfig::from_file(path) {
                pipeline.into_calibration_config()?
            } else {
                CalibrationConfig::from_file(path)?
            }
        } else {
            let open_dir = self.open_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --open-network-dir".to_string(),
                )
            })?;
            let ref_dir = self.reference_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --reference-network-dir".to_string(),
                )
            })?;
            let speeds_dir = self.reference_speeds_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --reference-speeds-dir".to_string(),
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
                time_bins: TimeBinSpec::default_six_bins(),
                reference_network: ReferenceNetworkConfig {
                    network_dir: ref_dir,
                    speed_files: ReferenceSpeedFilesConfig {
                        speeds_dir: Some(speeds_dir),
                        file_prefix: self.reference_file_prefix.clone(),
                        ..Default::default()
                    },
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
        if let Some(ref ref_dir) = self.reference_network_dir {
            cfg.reference_network.network_dir = ref_dir.clone();
        }
        if let Some(ref speeds_dir) = self.reference_speeds_dir {
            cfg.reference_network.speed_files.speeds_dir = Some(speeds_dir.clone());
        }
        if let Some(ref prefix) = self.reference_file_prefix {
            cfg.reference_network.speed_files.file_prefix = Some(prefix.clone());
        }
        if let Some(ref out_dir) = self.output_dir {
            cfg.output.output_dir = out_dir.clone();
        }
        if let Some(ref pbf_path) = self.osm_pbf_path {
            cfg.spatial_features.opportunities_source = Some(OpportunitiesSourceConfig::OsmPbf {
                pbf_path: pbf_path.clone(),
            });
        }
        if let Some(ref save_path) = self.save_bamcensus_csv {
            match cfg.spatial_features.census_source {
                Some(CensusSourceConfig::Bamcensus {
                    ref mut save_csv_path,
                    ..
                }) => {
                    *save_csv_path = Some(save_path.clone());
                }
                _ => {
                    cfg.spatial_features.census_source = Some(CensusSourceConfig::Bamcensus {
                        population_source: BamcensusPopulationConfig::default(),
                        employment_source: BamcensusLodesConfig::default(),
                        save_csv_path: Some(save_path.clone()),
                    });
                }
            }
        }

        Ok(cfg)
    }
}

/// CLI arguments for `bambam-congestion impute`.
///
/// Accepts the same unified `congestion.toml` (`CongestionPipelineConfig`) used for `calibrate`
/// or a standalone `ImputationConfig` via `--config <path.toml|.json>`.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct ImputeCliArgs {
    /// Path to a TOML or JSON configuration file (`congestion.toml`).
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

    /// Optional CSV (or `.csv.gz`) path to save the downloaded `bamcensus` dataset
    /// (`geoid,population,jobs,geometry`) when using `CensusSourceConfig::Bamcensus`.
    #[arg(long)]
    pub save_bamcensus_csv: Option<String>,
}

impl ImputeCliArgs {
    pub fn resolve_config(&self) -> Result<ImputationConfig, CongestionError> {
        let mut cfg = if let Some(ref config_path) = self.config {
            let path = Path::new(config_path);
            if let Ok(pipeline) = CongestionPipelineConfig::from_file(path) {
                pipeline.into_imputation_config()
            } else {
                ImputationConfig::from_file(path)?
            }
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
                time_bins: TimeBinSpec::default_six_bins(),
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
            cfg.spatial_features.opportunities_source = Some(OpportunitiesSourceConfig::OsmPbf {
                pbf_path: pbf_path.clone(),
            });
        }
        if let Some(ref save_path) = self.save_bamcensus_csv {
            match cfg.spatial_features.census_source {
                Some(CensusSourceConfig::Bamcensus {
                    ref mut save_csv_path,
                    ..
                }) => {
                    *save_csv_path = Some(save_path.clone());
                }
                _ => {
                    cfg.spatial_features.census_source = Some(CensusSourceConfig::Bamcensus {
                        population_source: BamcensusPopulationConfig::default(),
                        employment_source: BamcensusLodesConfig::default(),
                        save_csv_path: Some(save_path.clone()),
                    });
                }
            }
        }

        Ok(cfg)
    }
}

/// Unified pipeline configuration struct matching `congestion.toml`.
///
/// Can be passed directly to `bambam-congestion calibrate --config congestion.toml`,
/// `train_congestion_models.py --config congestion.toml`, and
/// `bambam-congestion impute --config congestion.toml` so that `[open_network]`,
/// `[[time_bins]]`, `[spatial_features]`, and `[artifacts]` stay in lockstep across
/// the entire workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CongestionPipelineConfig {
    /// Target Open Network (OSM or Overture Maps) in RouteE Compass format.
    #[serde(alias = "network")]
    pub open_network: OpenNetworkConfig,

    /// Explicit list of time-of-day bins (`[[time_bins]]`) with their names and indicator flags.
    /// Defaults to the 6 standard BAMBAM bins (`wd-am`, `wd-af`, `wd-pm`, `we-am`, `we-af`, `we-pm`)
    /// if omitted in TOML.
    #[serde(default = "TimeBinSpec::default_six_bins")]
    pub time_bins: Vec<TimeBinSpec>,

    /// Shared 0.5-mile spatial feature configuration (POIs, traffic controls, and Census densities).
    #[serde(default)]
    pub spatial_features: SpatialFeaturesConfig,

    /// Reference road network (in RouteE Compass format) with observed speed tables per time bin.
    /// Required for `calibrate`; optional when only running `impute`.
    #[serde(default, alias = "tomtom_network")]
    pub reference_network: Option<ReferenceNetworkConfig>,

    /// Spatial conflation parameters between `open_network` and `reference_network`.
    #[serde(default)]
    pub conflation: ConflationConfig,

    /// Shared artifact directories for calibration training tables, fitted models, and imputed outputs.
    #[serde(default)]
    pub artifacts: PipelineArtifactsConfig,
}

impl CongestionPipelineConfig {
    /// Loads a [`CongestionPipelineConfig`] from a `.toml` or `.json` file.
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
                "failed to deserialize CongestionPipelineConfig from '{}': {e}",
                filepath.display()
            ))
        })
    }

    /// Returns the programmatic list of configured time-bin names in order.
    pub fn time_bin_names(&self) -> Vec<&str> {
        TimeBinSpec::names(&self.time_bins)
    }

    /// Resolves the reference speed file path for each configured time bin.
    pub fn resolve_reference_speed_files(
        &self,
    ) -> Result<Vec<(TimeBinSpec, PathBuf)>, CongestionError> {
        let ref_net = self.reference_network.as_ref().ok_or_else(|| {
            CongestionError::Configuration(
                "missing `[reference_network]` section in congestion config".to_string(),
            )
        })?;
        ref_net.resolve_bin_speed_paths(&self.time_bins)
    }

    /// Converts this unified pipeline config into a [`CalibrationConfig`].
    pub fn into_calibration_config(self) -> Result<CalibrationConfig, CongestionError> {
        let reference_network = self.reference_network.ok_or_else(|| {
            CongestionError::Configuration(
                "`[reference_network]` is required in config when running `calibrate`".to_string(),
            )
        })?;
        Ok(CalibrationConfig {
            open_network: self.open_network,
            time_bins: self.time_bins,
            reference_network,
            conflation: self.conflation,
            spatial_features: self.spatial_features,
            output: CalibrationOutputConfig {
                output_dir: self.artifacts.calibration_dir,
                write_parquet: self.artifacts.write_parquet,
                write_csv: self.artifacts.write_csv,
            },
        })
    }

    /// Converts this unified pipeline config into an [`ImputationConfig`].
    pub fn into_imputation_config(self) -> ImputationConfig {
        let models = self.artifacts.resolve_model_paths();
        ImputationConfig {
            open_network: self.open_network,
            time_bins: self.time_bins,
            spatial_features: self.spatial_features,
            models,
            output_dir: self.artifacts.imputation_output_dir,
            write_parquet_summary: self.artifacts.write_parquet,
            write_csv_summary: self.artifacts.write_csv,
        }
    }
}

/// Shared artifact paths in `congestion.toml` (`[artifacts]`) connecting `calibrate`,
/// `train_congestion_models.py`, and `impute`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PipelineArtifactsConfig {
    /// Directory where `calibrate` writes `calibration_with_speedlimit.parquet` and
    /// `calibration_without_speedlimit.parquet`.
    #[serde(default = "default_calibration_dir")]
    pub calibration_dir: String,
    /// Directory where `train_congestion_models.py` writes (and `impute` reads)
    /// `xgboost_model_with_speedlimit.json` and `xgboost_model_without_speedlimit.json`.
    #[serde(default = "default_models_dir")]
    pub models_dir: String,
    /// Optional explicit override path for `with_speedlimit` model JSON.
    #[serde(default)]
    pub model_with_speedlimit_path: Option<String>,
    /// Optional explicit override path for `without_speedlimit` model JSON.
    #[serde(default)]
    pub model_without_speedlimit_path: Option<String>,
    /// Optional output directory for `impute` (defaults to `<open_network.network_dir>/<open_network.mode>`).
    #[serde(default)]
    pub imputation_output_dir: Option<String>,
    #[serde(default = "default_true")]
    pub write_parquet: bool,
    #[serde(default = "default_true")]
    pub write_csv: bool,
}

impl Default for PipelineArtifactsConfig {
    fn default() -> Self {
        Self {
            calibration_dir: default_calibration_dir(),
            models_dir: default_models_dir(),
            model_with_speedlimit_path: None,
            model_without_speedlimit_path: None,
            imputation_output_dir: None,
            write_parquet: true,
            write_csv: true,
        }
    }
}

impl PipelineArtifactsConfig {
    pub fn resolve_model_paths(&self) -> ImputationModelConfig {
        let base = Path::new(&self.models_dir);
        let with_sl = self.model_with_speedlimit_path.clone().unwrap_or_else(|| {
            base.join("xgboost_model_with_speedlimit.json")
                .to_string_lossy()
                .into_owned()
        });
        let without_sl = self
            .model_without_speedlimit_path
            .clone()
            .unwrap_or_else(|| {
                base.join("xgboost_model_without_speedlimit.json")
                    .to_string_lossy()
                    .into_owned()
            });
        ImputationModelConfig {
            model_with_speedlimit_path: with_sl,
            model_without_speedlimit_path: without_sl,
        }
    }
}

/// Top-level configuration for the congestion calibration workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationConfig {
    #[serde(alias = "network")]
    pub open_network: OpenNetworkConfig,
    #[serde(default = "TimeBinSpec::default_six_bins")]
    pub time_bins: Vec<TimeBinSpec>,
    #[serde(alias = "tomtom_network")]
    pub reference_network: ReferenceNetworkConfig,
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

    pub fn time_bin_names(&self) -> Vec<&str> {
        TimeBinSpec::names(&self.time_bins)
    }
}

/// Top-level configuration for the operating speed imputation workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImputationConfig {
    #[serde(alias = "network")]
    pub open_network: OpenNetworkConfig,
    #[serde(default = "TimeBinSpec::default_six_bins")]
    pub time_bins: Vec<TimeBinSpec>,
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

    pub fn time_bin_names(&self) -> Vec<&str> {
        TimeBinSpec::names(&self.time_bins)
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

/// Configuration for a Reference Road Network in RouteE Compass format with observed speed files per time bin.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReferenceNetworkConfig {
    /// Directory containing `edges-geometries-enumerated.txt.gz` (and optionally
    /// `edges-speed-profile-id-mapping-enumerated.txt.gz` and `speed-profile.csv.gz`).
    pub network_dir: String,
    /// Speed unit of the values in the reference speed files (defaults to `SpeedUnit::KPH`).
    #[serde(default = "default_kph_unit")]
    #[schemars(with = "String")]
    pub speed_unit: SpeedUnit,
    /// When `true`, only reference edges with time-varying speeds across time bins (i.e. not flat daily fallback)
    /// are used as ground-truth calibration targets.
    #[serde(default = "default_true", alias = "require_time_varying_trace")]
    pub require_time_varying_speeds: bool,
    /// Resolution strategy for locating the enumerated speed file for each configured `TimeBinSpec`:
    /// supports explicit per-bin filenames (`files = { "wd-am" = "..." }`) OR `speeds_dir` + `file_prefix` / `file_pattern`.
    #[serde(default, flatten)]
    pub speed_files: ReferenceSpeedFilesConfig,
}

impl Default for ReferenceNetworkConfig {
    fn default() -> Self {
        Self {
            network_dir: String::new(),
            speed_unit: default_kph_unit(),
            require_time_varying_speeds: true,
            speed_files: ReferenceSpeedFilesConfig::default(),
        }
    }
}

impl ReferenceNetworkConfig {
    /// Resolves the `(TimeBinSpec, PathBuf)` pairs for all configured `time_bins`.
    pub fn resolve_bin_speed_paths(
        &self,
        time_bins: &[TimeBinSpec],
    ) -> Result<Vec<(TimeBinSpec, PathBuf)>, CongestionError> {
        self.speed_files.resolve_for_bins(time_bins)
    }
}

/// Flexible configuration for locating reference speed files per time bin.
///
/// Users can either:
/// 1. Provide `speeds_dir` plus `file_prefix` (e.g. `file_prefix = "tomtom-natl"` -> `<speeds_dir>/tomtom-natl-<bin>.txt.gz`)
///    or `file_pattern` (e.g. `file_pattern = "speeds-{bin}.txt.gz"`), OR
/// 2. Provide explicit per-bin filenames in `files = { "wd-am" = "path/to/wd_am.txt.gz", ... }`
///    (or set `reference_speed_file` directly on each `[[time_bins]]` entry).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReferenceSpeedFilesConfig {
    /// Optional directory containing the reference speed files.
    #[serde(default)]
    pub speeds_dir: Option<String>,
    /// Optional filename prefix combined with each time bin's `name` (`<file_prefix>-<bin.name>.<file_extension>`).
    #[serde(default)]
    pub file_prefix: Option<String>,
    /// Optional filename pattern containing `{bin}` (or `{name}`), e.g. `"tomtom-natl-{bin}.txt.gz"`.
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// File extension used when constructing filenames from `file_prefix` (defaults to `"txt.gz"`).
    #[serde(default = "default_speed_file_ext")]
    pub file_extension: String,
    /// Optional explicit map from time-bin name (e.g. `"wd-am"` or `"wd_am"`) to file path.
    #[serde(default, alias = "period_speed_files")]
    pub files: HashMap<String, String>,
}

impl Default for ReferenceSpeedFilesConfig {
    fn default() -> Self {
        Self {
            speeds_dir: None,
            file_prefix: None,
            file_pattern: None,
            file_extension: default_speed_file_ext(),
            files: HashMap::new(),
        }
    }
}

impl ReferenceSpeedFilesConfig {
    /// Helper to construct a directory + prefix configuration (`<dir>/<prefix>-<bin>.txt.gz`).
    pub fn from_directory(dir: &Path, prefix: &str) -> Self {
        Self {
            speeds_dir: Some(dir.to_string_lossy().into_owned()),
            file_prefix: Some(prefix.to_string()),
            file_pattern: None,
            file_extension: default_speed_file_ext(),
            files: HashMap::new(),
        }
    }

    /// Resolves the concrete `PathBuf` for each `TimeBinSpec` in `time_bins`.
    pub fn resolve_for_bins(
        &self,
        time_bins: &[TimeBinSpec],
    ) -> Result<Vec<(TimeBinSpec, PathBuf)>, CongestionError> {
        if time_bins.is_empty() {
            return Err(CongestionError::Configuration(
                "`time_bins` cannot be empty".to_string(),
            ));
        }
        let base_dir = self.speeds_dir.as_deref().map(Path::new);
        let mut resolved = Vec::with_capacity(time_bins.len());

        for bin in time_bins {
            let underscore_key = bin.name.replace('-', "_");
            if let Some(explicit) = self
                .files
                .get(&bin.name)
                .or_else(|| self.files.get(&underscore_key))
            {
                let p = PathBuf::from(explicit);
                let full_path = match base_dir {
                    Some(dir) if !p.is_absolute() => dir.join(p),
                    _ => p,
                };
                resolved.push((bin.clone(), full_path));
            } else {
                let path = bin
                    .resolve_reference_speed_path(
                        base_dir,
                        self.file_prefix.as_deref(),
                        self.file_pattern.as_deref(),
                        &self.file_extension,
                    )
                    .map_err(CongestionError::Configuration)?;
                resolved.push((bin.clone(), path));
            }
        }
        Ok(resolved)
    }
}

/// Parameters controlling Open Network <-> Reference Network spatial conflation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConflationConfig {
    /// Corridor buffer distance in meters around each open-network edge (defaults to `5.0`).
    #[serde(default = "default_buffer_distance_meters")]
    pub buffer_distance_meters: f64,
    /// Maximum circular mean heading difference in degrees (`[0, 180]`, defaults to `45.0`).
    #[serde(default = "default_max_heading_diff_degrees")]
    pub max_heading_diff_degrees: f64,
    /// Minimum overlapping corridor length in meters required to accept a reference network match (defaults to `1.0`).
    #[serde(default = "default_min_overlap_meters")]
    pub min_overlap_meters: f64,
    /// Whether to cap observed reference speeds above `posted_speed + 10 mph` (or `max_class_speed + 5 mph`).
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
    /// Optional source for MEP land-use opportunity points (`overture_maps`, `osm_pbf`, `csv_points`, or `vertex_opportunities_csv`).
    /// Note: can be mixed and matched freely with any Open Network source (e.g. OMF road network + OSM PBF opportunities, or vice versa).
    #[serde(default)]
    pub opportunities_source: Option<OpportunitiesSourceConfig>,
    /// Optional override source for traffic controls (`traffic_signals`, `stop`, `crossing`).
    /// When `None`, traffic controls are automatically loaded from `<open_network>/vertices-complete.csv.gz`
    /// (which is produced by both `bambam-omf` and `bambam-osm`).
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
    /// Collect MEP land-use opportunities (`places` and `buildings`) directly from Overture Maps
    /// over the network's bounding box (via AWS S3 or a local OMF Parquet directory).
    OvertureMaps {
        /// Optional local directory containing Overture `places` / `buildings` Parquet files;
        /// when `None`, fetches directly from the public Overture Maps S3 bucket for the network's bbox.
        #[serde(default)]
        local_source_dir: Option<String>,
        /// Whether to also collect Overture `buildings` in addition to `places` (defaults to `true`).
        #[serde(default = "default_true")]
        include_buildings: bool,
    },
    /// Extract MEP land-use opportunities (and optionally traffic controls) directly from a local `.osm.pbf` file.
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

fn default_speed_file_ext() -> String {
    "txt.gz".to_string()
}

fn default_calibration_dir() -> String {
    "output/congestion_calibration".to_string()
}

fn default_models_dir() -> String {
    "output/congestion_models".to_string()
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
