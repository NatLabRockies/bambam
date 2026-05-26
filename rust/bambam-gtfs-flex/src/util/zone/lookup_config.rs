use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::consts;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ZoneLookupConfig {
    /// a file containing the complete list of GTFS-Flex zone ids enumerated by row number.
    /// each id must appear exactly once and should be a fully-qualified zone identifier.
    pub zone_ids_input_file: String,
    /// a processed collection of zone records
    pub zone_record_input_file: String,
    // /// optional column name for ZoneId values stored in the zonal GeoJSON input.
    // /// if not provided, "id" will be used.
    // pub zone_id_property: Option<String>,
    /// geometries for zones
    pub zone_geometry_input_file: String,
}

impl From<&Path> for ZoneLookupConfig {
    fn from(path: &Path) -> Self {
        let zone_ids_input_filepath = path.join(consts::ZONE_IDS_FILENAME);
        let zone_record_input_filepath = path.join(consts::RECORDS_FILENAME);
        let zone_geometry_input_filepath = path.join(consts::GEOMETRIES_FILENAME);
        let zone_ids_input_file = zone_ids_input_filepath.to_string_lossy().into_owned();
        let zone_record_input_file = zone_record_input_filepath.to_string_lossy().into_owned();
        let zone_geometry_input_file = zone_geometry_input_filepath.to_string_lossy().into_owned();
        ZoneLookupConfig {
            zone_ids_input_file,
            zone_record_input_file,
            zone_geometry_input_file,
        }
    }
}
