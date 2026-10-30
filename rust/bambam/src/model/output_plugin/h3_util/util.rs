use geo::{Geometry, LineString, Polygon};
use h3o::{CellIndex, Resolution};
use jsonpath_rust::JsonPath;
use routee_compass::{
    app::compass::CompassComponentError,
    plugin::{output::OutputPluginError, PluginError},
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::model::output_plugin::h3_util::{
    BoundaryGeometryFormat, DotDelimitedPath, H3UtilOutputPluginConfig,
};

#[derive(Debug, Clone)]
pub enum H3Util {
    /// reads an h3 identifier from some path in the output JSON and uses
    /// h3 cellToBoundary to write the h3
    /// see <https://h3geo.org/docs/api/indexing#celltoboundary>.
    H3BoundaryToGeometry {
        from: DotDelimitedPath,
        to: DotDelimitedPath,
        format: BoundaryGeometryFormat,
        overwrite: bool,
    },
    /// copies an h3 identifier from some JSONPath to another JSONPath,
    /// converting it to the declared parent resolution.
    H3ToParent {
        from: DotDelimitedPath,
        to: DotDelimitedPath,
        resolution: Resolution,
        overwrite: bool,
    },
}

impl H3Util {
    /// runs this H3 util on the Compass output, updating the output JSON in-place.
    pub fn apply(&self, output: &mut Value) -> Result<(), OutputPluginError> {
        match self {
            H3Util::H3BoundaryToGeometry {
                from,
                to,
                format,
                overwrite,
            } => {
                let from_jsonpath = from.to_jsonpath();
                let hex_idx = get_hex(output, &from_jsonpath).map_err(|e| {
                    let msg = format!("while running h3_boundary_to_geometry, {e}");
                    OutputPluginError::OutputPluginFailed(msg)
                })?;
                let polygon = h3_boundary_to_geometry(&hex_idx)?;
                let out_value = format.serialize(&polygon).map_err(|e| {
                    let msg = format!("while running h3_boundary_to_geometry, {e}");
                    OutputPluginError::OutputPluginFailed(msg)
                })?;
                set_value(output, to, out_value, *overwrite)
            }
            H3Util::H3ToParent {
                from,
                to,
                resolution,
                overwrite,
            } => {
                let from_jsonpath = from.to_jsonpath();
                let hex_idx = get_hex(output, &from_jsonpath).map_err(|e| {
                    let msg = format!("while running h3_to_parent, {e}");
                    OutputPluginError::OutputPluginFailed(msg)
                })?;
                let parent = h3_to_parent(&hex_idx, resolution)?;
                set_value(output, to, json![parent.to_string()], *overwrite)
            }
        }
    }
}

impl TryFrom<&H3UtilOutputPluginConfig> for H3Util {
    type Error = CompassComponentError;

    fn try_from(value: &H3UtilOutputPluginConfig) -> Result<Self, Self::Error> {
        match value {
            H3UtilOutputPluginConfig::H3BoundaryToGeometry {
                from,
                to,
                format,
                overwrite,
            } => {
                let from = DotDelimitedPath::try_from(from.clone()).map_err(|e| {
                    PluginError::BuildFailed(format!(
                        "while reading h3_boundary_to_geometry 'from' string: {e}"
                    ))
                })?;
                let to = DotDelimitedPath::try_from(to.clone()).map_err(|e| {
                    PluginError::BuildFailed(format!(
                        "while reading h3_boundary_to_geometry 'to' string: {e}"
                    ))
                })?;
                let format = format.clone().unwrap_or_default();
                let overwrite = overwrite.unwrap_or_default();
                Ok(H3Util::H3BoundaryToGeometry {
                    from,
                    to,
                    format,
                    overwrite,
                })
            }
            H3UtilOutputPluginConfig::H3ToParent {
                from,
                to,
                resolution,
                overwrite,
            } => {
                let from = DotDelimitedPath::try_from(from.clone()).map_err(|e| {
                    PluginError::BuildFailed(format!(
                        "while reading h3_to_parent 'from' string: {e}"
                    ))
                })?;
                let to = DotDelimitedPath::try_from(to.clone()).map_err(|e| {
                    PluginError::BuildFailed(format!("while reading h3_to_parent 'to' string: {e}"))
                })?;
                let resolution = h3o::Resolution::try_from(*resolution).map_err(|e| {
                    PluginError::BuildFailed(format!(
                        "while reading h3_to_parent 'resolution' number: {e}"
                    ))
                })?;
                let overwrite = overwrite.unwrap_or_default();

                Ok(H3Util::H3ToParent {
                    from,
                    to,
                    resolution,
                    overwrite,
                })
            }
        }
    }
}

/// turns a hex into its polygonal boundary in EPSG:4326 projection.
pub fn h3_boundary_to_geometry(hex_idx: &CellIndex) -> Result<Polygon, OutputPluginError> {
    // create boundary JSON
    let boundary: LineString = hex_idx.boundary().into();
    let polygon = Polygon::new(boundary, vec![]);

    Ok(polygon)
}

/// turns a hex into its parent hex at some parent resolution.
pub fn h3_to_parent(
    hex_idx: &CellIndex,
    resolution: &Resolution,
) -> Result<CellIndex, OutputPluginError> {
    let hex_idx_resolution = hex_idx.resolution();
    let parent = if hex_idx_resolution == *resolution {
        Ok(*hex_idx)
    } else {
        match hex_idx.parent(*resolution) {
            Some(parent) => Ok(parent),
            None => {
                let msg = format!("while running h3_to_parent, cannot find parent at finer resolution {resolution} for hex '{hex_idx}' with resolution {hex_idx_resolution}. You cannot get a parent at a finer (higher) resolution than the current cell.");
                Err(OutputPluginError::OutputPluginFailed(msg))
            }
        }
    }?;
    Ok(parent)
}

/// helper function to get a single value from a JSON value at some JSONPath
fn get_single_value<T: DeserializeOwned>(output: &Value, json_path: &str) -> Result<T, String> {
    let found_values = output
        .query(json_path)
        .map_err(|e| format!("failed to find value at '{json_path}': {e}"))?;
    let found_value: T = match found_values[..] {
        [from_value] => serde_json::from_value(from_value.clone()).map_err(|e| e.to_string()),
        _ => Err(format!(
            "invalid path, found more than one value at '{json_path}'"
        )),
    }?;
    Ok(found_value)
}

/// helper function to get an h3 hex from a JSON at some JSONPath
fn get_hex(output: &Value, json_path: &str) -> Result<CellIndex, String> {
    let hex_str: String =
        get_single_value(output, json_path).map_err(|e| format!("while getting h3 hex, {e}"))?;
    let hex_idx = hex_str
        .parse::<CellIndex>()
        .map_err(|e| format!("while parsing '{hex_str}' into h3 hex, {e}"))?;
    Ok(hex_idx)
}

/// helper function to write a value to the output at some json pointer location.
/// json pointers look like this: a/b/c
/// see <https://datatracker.ietf.org/doc/html/rfc6901> for the spec.
fn set_value(
    output: &mut Value,
    to: &DotDelimitedPath,
    value: Value,
    overwrite: bool,
) -> Result<(), OutputPluginError> {
    let to_pointer = to.to_jsonpointer();

    // break into parts delimited by forward slashes
    let parts: Vec<&str> = to_pointer.trim_start_matches('/').split('/').collect();

    // handle "root overwrite" case
    let overwrite_root = parts.is_empty() || (parts.len() == 1 && parts[0].is_empty()) && overwrite;
    if overwrite_root {
        let msg = format!("while writing to output, user provided path '{to}' to overwrite root, which is not supported.");
        return Err(OutputPluginError::OutputPluginFailed(msg));
    }

    let mut cursor = output;
    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1;

        if is_last {
            // Set the final value
            if let Some(obj) = cursor.as_object_mut() {
                if obj.contains_key(*part) && !overwrite {
                    let msg = format!(
                        "while writing to output, location '{part}' of path '{to}' already exists but overwrite is false"
                    );
                    return Err(OutputPluginError::OutputPluginFailed(msg));
                }
                obj.insert(part.to_string(), value);
                return Ok(());
            } else {
                let msg = format!(
                    "while writing to output, location '{part}' of path '{to}' is not an object"
                );
                return Err(OutputPluginError::OutputPluginFailed(msg));
            }
        } else {
            let cursor_obj = cursor.as_object_mut()
                .ok_or_else(|| {
                    let msg = format!("while writing to output, location '{part}' of path '{to}' is not a JSON object type");
                    OutputPluginError::OutputPluginFailed(msg)
                })?;
            // add child if it doesn't exist
            if !cursor_obj.contains_key(*part) {
                let _ = cursor_obj.insert(part.to_string(), json!({}));
            }

            // navigate down to the child
            if let Some(c) = cursor.get_mut(part) {
                cursor = c;
                continue;
            } else {
                return Err(OutputPluginError::OutputPluginFailed(
                    "internal error while writing to output".to_string(),
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use geo::CoordsIter;
    use geo_traits::LineStringTrait;

    use super::*;

    #[test]
    fn test_h3_boundary_to_geometry_valid_hex() {
        let hex_idx: CellIndex = "8a2a1072b59ffff".parse().unwrap();

        let result = h3_boundary_to_geometry(&hex_idx);

        assert!(result.is_ok());
        let polygon = result.unwrap();
        assert_eq!(polygon.exterior().coords_count(), 7); // H3 hexagons have 7 points (6 vertices + closing point)
        assert!(polygon.interiors().is_empty());
    }

    #[test]
    fn test_h3_to_parent_same_resolution() {
        let hex_idx: CellIndex = "8a2a1072b59ffff".parse().unwrap();
        let resolution = hex_idx.resolution();

        let result = h3_to_parent(&hex_idx, &resolution);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hex_idx);
    }

    #[test]
    fn test_h3_to_parent_coarser_resolution() {
        let hex_idx: CellIndex = "8a2a1072b59ffff".parse().unwrap();
        let parent_resolution = Resolution::try_from(8).unwrap();

        let result = h3_to_parent(&hex_idx, &parent_resolution);

        assert!(result.is_ok());
        let parent = result.unwrap();
        assert_eq!(parent.resolution(), parent_resolution);
    }

    #[test]
    fn test_h3_to_parent_finer_resolution_fails() {
        let hex_idx: CellIndex = "8a2a1072b59ffff".parse().unwrap();
        let finer_resolution = Resolution::try_from(11).unwrap();

        let result = h3_to_parent(&hex_idx, &finer_resolution);
        assert!(result.is_err());

        let result_msg = result.unwrap_err().to_string();
        assert!(result_msg.contains(
            "You cannot get a parent at a finer (higher) resolution than the current cell."
        ));
    }

    #[test]
    fn test_get_hex_valid() {
        let output = json!({
            "location": {
                "hex": "8a2a1072b59ffff"
            }
        });

        let result = get_hex(&output, "$.location.hex");

        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), "8a2a1072b59ffff");
    }

    #[test]
    fn test_get_hex_invalid_format() {
        let output = json!({
            "location": {
                "hex": "invalid_hex"
            }
        });

        let result = get_hex(&output, "$.location.hex");

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("while parsing"));
    }

    #[test]
    fn test_set_value_valid_path_overwrite() {
        let mut output = json!({
            "result": {
                "geometry": null
            }
        });
        let to = DotDelimitedPath::try_from("result.geometry".to_string())
            .expect("test invariant failed");
        let value = json!({"type": "Polygon"});

        let result = set_value(&mut output, &to, value, true);

        assert!(result.is_ok());
        assert_eq!(output["result"]["geometry"], json!({"type": "Polygon"}));
    }

    #[test]
    fn test_set_value_valid_path_no_overwrite() {
        let mut output = json!({
            "result": {
                "geometry": null
            }
        });
        let to = DotDelimitedPath::try_from("result.geometry".to_string())
            .expect("test invariant failed");
        let value = json!({"type": "Polygon"});

        let result = set_value(&mut output, &to, value, false);

        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("overwrite is false"));
    }

    #[test]
    fn test_set_value_attempts_to_overwrite_root() {
        let mut output = json!({});
        let to = DotDelimitedPath::try_from("".to_string()).expect("test invariant failed");
        let value = json!("value");

        let result = set_value(&mut output, &to, value, true);

        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("overwrite root"));
    }

    #[test]
    fn test_set_value_root_level_new_key() {
        let mut output = json!({
            "existing_key": "value"
        });
        let to = DotDelimitedPath::try_from("geometry".to_string()).expect("test invariant failed");
        let value = json!({"type": "Polygon", "coordinates": []});

        let result = set_value(&mut output, &to, value.clone(), false);

        assert!(result.is_ok());
        assert_eq!(output["geometry"], value);
        assert_eq!(output["existing_key"], "value"); // ensure existing data is preserved
    }

    #[test]
    fn test_set_value_creates_nested_path() {
        let mut output = json!({
            "existing": "data"
        });
        let to = DotDelimitedPath::try_from("new.nested.path".to_string())
            .expect("test invariant failed");
        let value = json!("test_value");

        let result = set_value(&mut output, &to, value, false);

        assert!(result.is_ok());
        assert_eq!(output["new"]["nested"]["path"], "test_value");
        assert_eq!(output["existing"], "data");
    }

    #[test]
    fn test_set_value_with_array_in_path() {
        let mut output = json!({
            "existing": [
                "data"
            ]
        });
        let to =
            DotDelimitedPath::try_from("existing.data".to_string()).expect("test invariant failed");
        let value = json!("test_value");

        let result = set_value(&mut output, &to, value, false);

        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("is not an object"))
    }

    #[test]
    fn test_set_value_overwrites_existing_root_key() {
        let mut output = json!({
            "geometry": "old_value"
        });
        let to = DotDelimitedPath::try_from("geometry".to_string()).expect("test invariant failed");
        let value = json!({"type": "Polygon"});

        let result = set_value(&mut output, &to, value.clone(), true);

        assert!(result.is_ok());
        assert_eq!(output["geometry"], value);
    }

    #[test]
    fn test_h3_boundary_to_geometry_apply() {
        let mut output = json!({
            "location": {
                "hex": "8a2a1072b59ffff",
                "geometry": null
            }
        });

        let h3_util = H3Util::H3BoundaryToGeometry {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.geometry".to_string()).unwrap(),
            format: BoundaryGeometryFormat::GeoJson,
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_ok());
        assert!(output["location"]["geometry"].is_object());
        assert_eq!(output["location"]["geometry"]["type"], "Feature");
        assert_eq!(
            output["location"]["geometry"]["geometry"]["type"],
            "Polygon"
        );
        assert!(output["location"]["geometry"]["geometry"]["coordinates"].is_array());
    }

    #[test]
    fn test_h3_boundary_to_geometry_apply_wkt() {
        let mut output = json!({
            "location": {
                "hex": "8a2a1072b59ffff",
                "wkt": null
            }
        });

        let h3_util = H3Util::H3BoundaryToGeometry {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.wkt".to_string()).unwrap(),
            format: BoundaryGeometryFormat::Wkt,
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_ok());
        assert!(output["location"]["wkt"].is_string());
        let wkt_str = output["location"]["wkt"].as_str().unwrap();
        assert!(wkt_str.starts_with("POLYGON"));
    }

    #[test]
    fn test_h3_boundary_to_geometry_apply_invalid_hex() {
        let mut output = json!({
            "location": {
                "hex": "invalid_hex",
                "geometry": null
            }
        });

        let h3_util = H3Util::H3BoundaryToGeometry {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.geometry".to_string()).unwrap(),
            format: BoundaryGeometryFormat::GeoJson,
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_err());
    }

    #[test]
    fn test_h3_boundary_to_geometry_apply_missing_path() {
        let mut output = json!({
            "location": {}
        });

        let h3_util = H3Util::H3BoundaryToGeometry {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.geometry".to_string()).unwrap(),
            format: BoundaryGeometryFormat::GeoJson,
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_err());
    }

    #[test]
    fn test_h3_to_parent_apply() {
        let mut output = json!({
            "location": {
                "hex": "8a2a1072b59ffff",
                "parent_hex": null
            }
        });

        let h3_util = H3Util::H3ToParent {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.parent_hex".to_string()).unwrap(),
            resolution: Resolution::try_from(8).unwrap(),
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_ok());
        assert!(output["location"]["parent_hex"].is_string());
        let parent_hex = output["location"]["parent_hex"].as_str().unwrap();
        let parent_idx: CellIndex = parent_hex.parse().unwrap();
        assert_eq!(parent_idx.resolution(), Resolution::try_from(8).unwrap());
    }

    #[test]
    fn test_h3_to_parent_apply_same_resolution() {
        let mut output = json!({
            "location": {
                "hex": "8a2a1072b59ffff",
                "parent_hex": null
            }
        });

        let hex_idx: CellIndex = "8a2a1072b59ffff".parse().unwrap();
        let resolution = hex_idx.resolution();

        let h3_util = H3Util::H3ToParent {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.parent_hex".to_string()).unwrap(),
            resolution,
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_ok());
        assert_eq!(output["location"]["parent_hex"], "8a2a1072b59ffff");
    }

    #[test]
    fn test_h3_to_parent_apply_invalid_resolution() {
        let mut output = json!({
            "location": {
                "hex": "8a2a1072b59ffff",
                "parent_hex": null
            }
        });

        let h3_util = H3Util::H3ToParent {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.parent_hex".to_string()).unwrap(),
            resolution: Resolution::try_from(11).unwrap(), // finer than the hex resolution
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_err());
    }

    #[test]
    fn test_h3_to_parent_apply_missing_hex() {
        let mut output = json!({
            "location": {}
        });

        let h3_util = H3Util::H3ToParent {
            from: DotDelimitedPath::try_from("location.hex".to_string()).unwrap(),
            to: DotDelimitedPath::try_from("location.parent_hex".to_string()).unwrap(),
            resolution: Resolution::try_from(8).unwrap(),
            overwrite: true,
        };

        let result = h3_util.apply(&mut output);

        assert!(result.is_err());
    }
}
