use geo::Polygon;
use geozero::{CoordDimensions, ToJson, ToWkb, ToWkt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryGeometryFormat {
    Wkt,
    #[default]
    Wkb,
    GeoJson,
}

impl TryFrom<&str> for BoundaryGeometryFormat {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().trim() {
            "wkt" => Ok(Self::Wkt),
            "wkb" => Ok(Self::Wkb),
            "geojson" => Ok(Self::GeoJson),
            _ => Err(format!("unknown boundary geometry format '{value}'")),
        }
    }
}

impl std::fmt::Display for BoundaryGeometryFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BoundaryGeometryFormat::Wkt => "wkt",
            BoundaryGeometryFormat::Wkb => "wkb",
            BoundaryGeometryFormat::GeoJson => "geojson",
        };
        write!(f, "{s}")
    }
}

impl BoundaryGeometryFormat {
    pub fn serialize(&self, boundary: &Polygon) -> Result<Value, String> {
        let geom: geo::Geometry<f64> = boundary.clone().into();
        match self {
            BoundaryGeometryFormat::Wkt => {
                let out = geom.to_wkt().map_err(|e| e.to_string())?;
                Ok(json![out])
            }
            BoundaryGeometryFormat::Wkb => {
                // Convert to WKB
                let out_bytes = geom
                    .to_wkb(CoordDimensions::xy())
                    .map_err(|e| e.to_string())?;

                // Write to query as uppercase hex string
                let output = out_bytes
                    .iter()
                    .map(|b| format!("{b:02X?}"))
                    .collect::<Vec<String>>()
                    .join("");

                Ok(json![output])
            }
            BoundaryGeometryFormat::GeoJson => {
                let geom_json_str = geom.to_json().map_err(|e| e.to_string())?;
                let geom_json: Value = serde_json::from_str(&geom_json_str)
                    .map_err(|e| e.to_string())?;
                let result = json!({
                    "type": "Feature",
                    "bbox": null,
                    "geometry": geom_json,
                    "id": null,
                    "properties": null
                });
                Ok(result)
            }
        }
    }
}
