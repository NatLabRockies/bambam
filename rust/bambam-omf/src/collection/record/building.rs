use geo::Geometry;
use serde::{Deserialize, Serialize};

use super::{geometry_wkb_codec, OvertureMapsBbox, OvertureRecord};
use crate::collection::OvertureMapsCollectionError;

/// OvertureMaps Buildings record schema.
///
/// NOTE: The fields defined on this struct directly drive the Parquet [`ProjectionMask`]
/// used when reading Overture GeoParquet files (`FieldRef::from_type::<BuildingsRecord>()`).
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildingsRecord {
    #[serde(default)]
    id: Option<String>,
    #[serde(with = "geometry_wkb_codec", default)]
    geometry: Option<Geometry<f32>>,
    #[serde(default)]
    bbox: OvertureMapsBbox,
    #[serde(default)]
    version: i32,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    class: Option<String>,
    // Unused Overture Buildings columns omitted from projection:
    // sources: Option<Vec<Option<OvertureMapsSource>>>,
    // names: Option<OvertureMapsNames>,
    // level: Option<i32>,
    // has_parts: Option<bool>,
    // is_underground: Option<bool>,
    // height: Option<f64>,
    // num_floors: Option<i32>,
    // num_floors_underground: Option<i32>,
    // min_height: Option<f64>,
    // min_floor: Option<i32>,
    // facade_color: Option<String>,
    // facade_material: Option<String>,
    // roof_material: Option<String>,
    // roof_shape: Option<String>,
    // roof_direction: Option<f64>,
    // roof_orientation: Option<String>,
    // roof_color: Option<String>,
}

impl BuildingsRecord {
    pub fn get_class(&self) -> Option<String> {
        self.class.clone()
    }

    pub fn get_geometry(&self) -> Option<Geometry<f32>> {
        self.geometry.clone()
    }
}

impl TryFrom<OvertureRecord> for BuildingsRecord {
    type Error = OvertureMapsCollectionError;

    fn try_from(value: OvertureRecord) -> Result<Self, Self::Error> {
        match value {
            OvertureRecord::Buildings(record) => Ok(record),
            _ => Err(OvertureMapsCollectionError::DeserializeTypeError(format!(
                "Cannot transform record {value:#?} into BuildingRecord"
            ))),
        }
    }
}
