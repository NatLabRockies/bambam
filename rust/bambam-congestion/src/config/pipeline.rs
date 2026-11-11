use std::path::{Path, PathBuf};

use config::{Config, File};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::CongestionError, taxonomy::TimeBinSpec};

use super::{
    CalibrationConfig, CalibrationOutputConfig, ConflationConfig, ImputationConfig,
    ImputationModelConfig, OpenNetworkConfig, ReferenceNetworkConfig, SpatialFeaturesConfig,
};

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

fn default_calibration_dir() -> String {
    "output/congestion_calibration".to_string()
}

fn default_models_dir() -> String {
    "output/congestion_models".to_string()
}

fn default_true() -> bool {
    true
}
