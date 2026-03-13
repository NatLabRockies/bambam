use clap::ValueEnum;
use geo::Geometry;
use geozero::{wkt::Wkt as WktReader, ToGeo};
use routee_compass_core::config::ConfigJsonExtensions;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ExtentFormat {
    /// user extent field to be treated as a WKT
    #[default]
    Wkt,
    // future extention points:
    // Wkb,
    // GeoJson,
}

impl ExtentFormat {
    /// Reads `extent` key in the root of the input and parses it into [`Geometry`].
    /// Currently only implements WKT format
    pub fn get_extent(&self, input: &mut serde_json::Value) -> Result<Geometry, String> {
        match self {
            ExtentFormat::Wkt => {
                let wkt_str = input
                    .get_config_serde::<String>(&super::EXTENT, &"<root>")
                    .map_err(|e| {
                        format!(
                            "failure reading extent, are you sure you submitted a valid WKT?: {e}"
                        )
                    })?;
                WktReader(wkt_str.as_str())
                    .to_geo()
                    .map_err(|e| format!("failure converting wkt to geo: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extent_format_wkt_polygon() {
        let fmt = ExtentFormat::Wkt;
        let mut input = json!({ "extent": "POLYGON((0 0, 1 0, 1 1, 0 1, 0 0))" });
        let result = fmt
            .get_extent(&mut input)
            .expect("should parse WKT polygon");
        assert!(matches!(result, Geometry::Polygon(_)));
    }

    #[test]
    fn test_extent_format_wkt_multipolygon() {
        let fmt = ExtentFormat::Wkt;
        let mut input = json!({ "extent": "MULTIPOLYGON(((0 0, 1 0, 1 1, 0 1, 0 0)))" });
        let result = fmt
            .get_extent(&mut input)
            .expect("should parse WKT multipolygon");
        assert!(matches!(result, Geometry::MultiPolygon(_)));
    }

    #[test]
    fn test_extent_format_wkt_invalid() {
        let fmt = ExtentFormat::Wkt;
        let mut input = json!({ "extent": "NOT VALID WKT" });
        let result = fmt.get_extent(&mut input);
        assert!(result.is_err(), "expected error for invalid WKT");
    }
}
