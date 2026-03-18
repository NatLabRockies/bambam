use geo::{Geometry, MapCoords, TryConvert};
use geozero::{
    geojson::GeoJsonString, wkt::Wkt as WktReader, CoordDimensions, ToGeo, ToJson, ToWkb, ToWkt,
};
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
                // Parse the JSON and extract geometry, handling both raw geometry and Feature format
                let parsed: serde_json::Value = serde_json::from_str(geojson_str).map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failure parsing GeoJSON string: {e}, found: {value:?}"
                    ))
                })?;
                let geom_json = if parsed["type"] == "Feature" {
                    serde_json::to_string(&parsed["geometry"]).map_err(|e| {
                        OutputPluginError::OutputPluginFailed(format!(
                            "failure extracting geometry from GeoJSON Feature: {e}"
                        ))
                    })?
                } else {
                    geojson_str.to_string()
                };
                let geometry_f64 = GeoJsonString(geom_json).to_geo().map_err(|e| {
                    OutputPluginError::OutputPluginFailed(format!(
                        "failure parsing GeoJSON geometry due to: {e}, found: {value:?}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use geo::polygon;
    use serde_json::json;

    fn sample_polygon_f32() -> Geometry<f32> {
        Geometry::Polygon(polygon![
            (x: 0.0f32, y: 0.0f32),
            (x: 1.0f32, y: 0.0f32),
            (x: 1.0f32, y: 1.0f32),
            (x: 0.0f32, y: 1.0f32),
            (x: 0.0f32, y: 0.0f32),
        ])
    }

    #[test]
    fn test_serialize_wkt_roundtrip() {
        let fmt = IsochroneOutputFormat::Wkt;
        let geom = sample_polygon_f32();
        let serialized = fmt
            .serialize_geometry(&geom)
            .expect("wkt serialization failed");
        assert!(
            serialized.starts_with("POLYGON"),
            "expected WKT POLYGON, got: {serialized}"
        );
        let deserialized = fmt
            .deserialize_geometry(&json!(serialized))
            .expect("wkt deserialization failed");
        // verify the geometry type is preserved
        assert!(matches!(deserialized, Geometry::Polygon(_)));
    }

    #[test]
    fn test_serialize_wkb_roundtrip() {
        let fmt = IsochroneOutputFormat::Wkb;
        let geom = sample_polygon_f32();
        let serialized = fmt
            .serialize_geometry(&geom)
            .expect("wkb serialization failed");
        // WKB is hex-encoded
        assert!(serialized.len() > 0, "expected non-empty WKB hex string");
        let deserialized = fmt
            .deserialize_geometry(&json!(serialized))
            .expect("wkb deserialization failed");
        assert!(matches!(deserialized, Geometry::Polygon(_)));
    }

    #[test]
    fn test_serialize_geojson_roundtrip() {
        let fmt = IsochroneOutputFormat::GeoJson;
        let geom = sample_polygon_f32();
        let serialized = fmt
            .serialize_geometry(&geom)
            .expect("geojson serialization failed");
        let parsed: serde_json::Value =
            serde_json::from_str(&serialized).expect("result should be valid json");
        assert_eq!(parsed["type"], "Feature");
        assert_eq!(parsed["geometry"]["type"], "Polygon");
        let deserialized = fmt
            .deserialize_geometry(&json!(serialized))
            .expect("geojson deserialization failed");
        assert!(matches!(deserialized, Geometry::Polygon(_)));
    }

    #[test]
    fn test_empty_geometry_wkt() {
        let fmt = IsochroneOutputFormat::Wkt;
        let result = fmt.empty_geometry().expect("empty geometry wkt failed");
        assert!(
            result.contains("POLYGON"),
            "expected WKT POLYGON, got: {result}"
        );
    }

    #[test]
    fn test_deserialize_wkt_invalid_input() {
        let fmt = IsochroneOutputFormat::Wkt;
        let result = fmt.deserialize_geometry(&json!("NOT VALID WKT!!"));
        assert!(result.is_err(), "expected error for invalid WKT");
    }

    #[test]
    fn test_deserialize_wkb_invalid_hex() {
        let fmt = IsochroneOutputFormat::Wkb;
        let result = fmt.deserialize_geometry(&json!("ZZZNOTVALIDHEX"));
        assert!(result.is_err(), "expected error for invalid WKB hex");
    }
}
