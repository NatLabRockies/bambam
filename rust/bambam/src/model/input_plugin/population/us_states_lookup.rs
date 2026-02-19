use bambam_core::util::polygonal_rtree::PolygonalRTree;
use bamcensus_core::model::identifier::Geoid;
use flate2::read::GzDecoder;
use geo::Geometry;
use std::io::Read;

pub fn load() -> Result<PolygonalRTree<Geoid>, String> {
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
    let json = buf.parse::<geojson::GeoJson>().map_err(|e| {
        format!("failure reading tl_2023_us_state_in_tiger_lines.geojson.gz as geojson: {e}")
    })?;
    let tree_nodes = match json {
        geojson::GeoJson::FeatureCollection(feature_collection) => {
            let n_features = feature_collection.features.len();
            let mut tree_nodes = Vec::with_capacity(n_features);
            for feature in feature_collection {
                // rjf 2024-09-26: we cannot currently trust using Geoid's Deserialize impl.
                let geoid_str = feature
                    .property("GEOID")
                    .ok_or_else(|| String::from("no GEOID in feature!"))?
                    .as_str()
                    .ok_or_else(|| String::from("cannot read feature GEOID as string"))?;
                let geoid: Geoid = geoid_str
                    .try_into()
                    .map_err(|e| format!("failure decoding GEOID in feature: {e}"))?;
                let geom_json = feature
                    .geometry
                    .ok_or_else(|| format!("no geometry in GEOID {geoid}"))?;
                let geometry: Geometry = geom_json.try_into().map_err(|e| {
                    format!(
                        "failure decoding GeoJson geometry to geo-types for GEOID {geoid}: {e}"
                    )
                })?;
                tree_nodes.push((geometry, geoid));
            }
            Ok(tree_nodes)
        }
        _ => Err(String::from(
            "state reference file tl_2023_us_state_in_tiger_lines.geojson.gz expected to be a FeatureCollection",
        )),
    }?;
    let tree = PolygonalRTree::new(tree_nodes)?;
    Ok(tree)
}
