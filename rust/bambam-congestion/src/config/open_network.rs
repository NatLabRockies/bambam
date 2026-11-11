use routee_compass_core::model::unit::SpeedUnit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

fn default_drive_mode() -> String {
    "drive".to_string()
}

fn default_kph_unit() -> SpeedUnit {
    SpeedUnit::KPH
}

fn default_min_speed_limit_mph() -> f64 {
    15.0
}
