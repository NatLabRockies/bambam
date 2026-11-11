use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use routee_compass_core::model::unit::SpeedUnit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{error::CongestionError, taxonomy::TimeBinSpec};

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

fn default_kph_unit() -> SpeedUnit {
    SpeedUnit::KPH
}

fn default_true() -> bool {
    true
}

fn default_speed_file_ext() -> String {
    "txt.gz".to_string()
}
