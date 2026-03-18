use bamcensus_core::model::identifier::Geoid;
use flate2::read::GzDecoder;
use geo::Geometry;
use geozero::{geojson::GeoJsonString, ToGeo};
use routee_compass_core::util::geo::PolygonalRTree;
use std::io::Read;

pub fn load() -> Result<PolygonalRTree<f64, Geoid>, String> {
    // source: https://www2.census.gov/geo/tiger/TIGER2023/STATE/tl_2023_us_state.zip
    // shapefile loaded in GeoPandas, written to file using df.to_json() method. removed
    // any entries with FIPS Geoids that do not occur in the Tiger/LINES datasets,
    // then gzip'd the resulting GeoJSON.
    let state_file = include_bytes!("tl_2023_us_state_in_tiger_lines.geojson.gz");
    let mut file = GzDecoder::new(&state_file[..]);
    let mut buf = String::new();
    let _ = file.read_to_string(&mut buf).map_err(|e| {
        format!("failure reading tl_2023_us_state_in_tiger_lines.geojson.gz into memory: {e}")
    })?;
    let json: serde_json::Value = serde_json::from_str(&buf).map_err(|e| {
        format!("failure reading tl_2023_us_state_in_tiger_lines.geojson.gz as geojson: {e}")
    })?;
    let features = json["features"].as_array().ok_or_else(|| {
        String::from(
            "state reference file tl_2023_us_state_in_tiger_lines.geojson.gz expected to be a FeatureCollection",
        )
    })?;
    let n_features = features.len();
    let mut tree_nodes = Vec::with_capacity(n_features);
    for feature in features {
        // rjf 2024-09-26: we cannot currently trust using Geoid's Deserialize impl.
        let geoid_str = feature["properties"]["GEOID"]
            .as_str()
            .ok_or_else(|| String::from("no GEOID in feature!"))?;
        let geoid: Geoid = geoid_str
            .try_into()
            .map_err(|e| format!("failure decoding GEOID in feature: {e}"))?;
        let geometry_json = serde_json::to_string(&feature["geometry"])
            .map_err(|e| format!("failure serializing geometry for GEOID {geoid}: {e}"))?;
        let geometry: Geometry = GeoJsonString(geometry_json).to_geo().map_err(|e| {
            format!("failure decoding GeoJson geometry to geo-types for GEOID {geoid}: {e}")
        })?;
        tree_nodes.push((geometry, geoid));
    }
    let tree = PolygonalRTree::new(tree_nodes)?;
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use geozero::ToGeo;

    use super::*;

    /// Test that the embedded GeoJSON state file loads successfully
    /// and produces a non-empty RTree with valid Polygon/MultiPolygon geometries.
    #[test]
    fn test_load_returns_non_empty_tree() {
        let tree = load().expect("load() should succeed");
        // The embedded state file has at least one state: test a known intersection
        // with a point in the continental US (Kansas City area)
        let point = geo::Geometry::Point(geo::Point::new(-94.5786, 39.0997));
        let intersecting = tree
            .intersection(&point)
            .expect("intersection should succeed");
        assert!(
            intersecting.count() > 0,
            "expected at least one state to contain a Kansas City area point"
        );
    }

    /// Test that GeoJsonString can parse a simple GeoJSON geometry via geozero
    #[test]
    fn test_geojson_string_to_geo() {
        let geojson = GeoJsonString(
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#.to_string(),
        );
        let geom = geojson.to_geo().expect("should parse GeoJSON polygon");
        assert!(matches!(geom, geo::Geometry::Polygon(_)));
    }
}
