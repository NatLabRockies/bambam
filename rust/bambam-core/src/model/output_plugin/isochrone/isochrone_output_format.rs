use geo::{Geometry, MapCoords, TryConvert};
use geo_traits::to_geo::ToGeoGeometry;
use geozero::{geojson::GeoJsonString, wkt::Wkt as WktReader, CoordDimensions, ToGeo, ToJson, ToWkb, ToWkt};
use routee_compass::plugin::output::OutputPluginError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum IsochroneOutputFormat {
    Wkt,
    Wkb,
    GeoJson,
}

impl IsochroneOutputFormat {
    pub fn empty_geometry(&self) -> Result<String, OutputPluginError> {
        let empty: Geometry<f32> = Geometry::Polygon(geo::polygon![]);
        self.serialize_geometry(&empty)
    }

    pub fn deserialize_geometry(&self, value: &Value) -> Result<Geometry<f32>, OutputPluginError> {
        match self {
            IsochroneOutputFormat::Wkt => {
                let wkt = value.as_str().ok_or_else(|| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "expected WKT string for geometry deserialization, found: {value:?}"
                    ))
                })?;
                let geometry_f64 = WktReader(wkt).to_geo().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failure deserializing WKT geometry from output row due to: {e} - WKT string: \"{wkt}\""
                    ))
                })?;
                try_convert_f32(&geometry_f64)
            }
            IsochroneOutputFormat::Wkb => {
                let wkb_str = value.as_str().ok_or_else(|| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "expected WKB string for geometry deserialization, found: {value:?}"
                    ))
                })?;
                // Decode hex string to bytes
                let wkb_bytes = hex::decode(wkb_str).map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failed to decode WKB hex string: {e} - WKB string: \"{wkb_str}\""
                    ))
                })?;
                // Read geometry as f64 via geozero, then convert to f32
                let geometry_f64 = geozero::wkb::Wkb(wkb_bytes).to_geo().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failure deserializing WKB geometry from output row due to: {e} - WKB string: \"{wkb_str}\""
                    ))
                })?;
                try_convert_f32(&geometry_f64)
            }
            IsochroneOutputFormat::GeoJson => {
                let geojson_str = value.as_str().ok_or_else(|| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "expected string for geometry deserialization, found: {value:?}"
                    ))
                })?;
                let geometry_f64 = GeoJsonString(geojson_str.to_string()).to_geo().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failure parsing GeoJSON from geometry string due to: {e}, found: {value:?}"
                    ))
                })?;
                try_convert_f32(&geometry_f64)
            }
        }
    }

    pub fn serialize_geometry(
        &self,
        geometry: &Geometry<f32>,
    ) -> Result<String, OutputPluginError> {
        match self {
            IsochroneOutputFormat::Wkt => {
                let geom: Geometry<f64> = geometry.try_convert().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "unable to convert geometry from f32 to f64: {e}"
                    ))
                })?;
                geom.to_wkt().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failed to write geometry as WKT: {e}"
                    ))
                })
            }
            IsochroneOutputFormat::Wkb => {
                let geom: Geometry<f64> = geometry.try_convert().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "unable to convert geometry from f32 to f64: {e}"
                    ))
                })?;
                let out_bytes = geom.to_wkb(CoordDimensions::xy()).map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failed to write geometry as WKB: {e}"
                    ))
                })?;
                Ok(out_bytes
                    .iter()
                    .map(|b| format!("{b:02X?}"))
                    .collect::<Vec<String>>()
                    .join(""))
            }
            IsochroneOutputFormat::GeoJson => {
                let geom: Geometry<f64> = geometry.try_convert().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "unable to convert geometry from f32 to f64: {e}"
                    ))
                })?;
                let geom_json_str = geom.to_json().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failed to serialize geometry as GeoJSON: {e}"
                    ))
                })?;
                let geom_json: serde_json::Value = serde_json::from_str(&geom_json_str)?;
                let feature = serde_json::json!({
                    "type": "Feature",
                    "bbox": null,
                    "geometry": geom_json,
                    "id": null,
                    "properties": null
                });
                Ok(feature.to_string())
            }
        }
    }
}

fn try_convert_f32(g: &Geometry<f64>) -> Result<Geometry<f32>, OutputPluginError> {
    let (min, max) = (f32::MIN as f64, f32::MAX as f64);
    g.try_map_coords(|geo::Coord { x, y }| {
        if x < min || max < x {
            Err(OutputPluginError::OutputPluginFailed(format!(
                "could not express x value '{x}' as f32, exceeds range of possible values [{min}, {max}]"
            )))
        } else if y < min || max < y {
            Err(OutputPluginError::OutputPluginFailed(format!(
                "could not express y value '{y}' as f32, exceeds range of possible values [{min}, {max}]"
            )))
        } else {
            let x32 = x as f32;
            let y32 = y as f32;
            Ok(geo::Coord { x: x32, y: y32 })
        }
    })
}
