use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ZoneLookupConfig {
    /// a GeoJSON collection of zone records
    pub zone_record_input_file: String,
    /// optional column name for ZoneId values stored in the zonal GeoJSON input.
    /// if not provided, "zone_id" will be used.
    #[serde(default = "default_zone_id_column")]
    pub zone_id_column: String,
    /// geometries for zones
    pub zone_geometry_input_file: String,
}

fn default_zone_id_column() -> String {
    "zone_id".to_string()
}
