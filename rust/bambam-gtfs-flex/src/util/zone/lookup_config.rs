use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ZoneLookupConfig {
    /// a GeoJSON collection of zone records
    pub zone_record_input_file: String,
    /// optional column name for ZoneId values stored in the zonal GeoJSON input.
    /// if not provided, "id" will be used.
    pub zone_id_property: Option<String>,
    /// geometries for zones
    pub zone_geometry_input_file: String,
}
