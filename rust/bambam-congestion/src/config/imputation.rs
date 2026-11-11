use std::path::Path;

use config::{Config, File};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::CongestionError, taxonomy::TimeBinSpec};

use super::{OpenNetworkConfig, SpatialFeaturesConfig};

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

/// Model artifact paths for `bambam-congestion impute`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImputationModelConfig {
    /// Path to the calibrated `with_speedlimit` XGBoost JSON model (`model.save_model("...json")`).
    pub model_with_speedlimit_path: String,
    /// Path to the calibrated `without_speedlimit` XGBoost JSON model (`model.save_model("...json")`).
    pub model_without_speedlimit_path: String,
}

fn default_true() -> bool {
    true
}
