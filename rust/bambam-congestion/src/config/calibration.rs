use std::path::Path;

use config::{Config, File};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::CongestionError, taxonomy::TimeBinSpec};

use super::{ConflationConfig, OpenNetworkConfig, ReferenceNetworkConfig, SpatialFeaturesConfig};

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

fn default_true() -> bool {
    true
}
