use serde_json::json;
use geozero::ToWkt;

/// creates a JSON value to return as part of the "grid", which is
/// really just a RouteE Compass query.
pub fn create_grid_row(
    grid_id: String,
    x: f64,
    y: f64,
    geometry: &geo::Geometry,
    template: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut output_value = template.clone();
    let output_map = output_value.as_object_mut().ok_or_else(|| {
        format!("internal error, cannot build from template that is not JSON mappable: {template}")
    })?;
    output_map.insert(super::GRID_ID.to_string(), json![grid_id]);
    output_map.insert(super::ORIGIN_X.to_string(), json![x]);
    output_map.insert(super::ORIGIN_Y.to_string(), json![y]);
    output_map.insert(
        super::GEOMETRY.to_string(),
        json![geometry.to_wkt().map_err(|e| format!("failure serializing geometry as WKT: {e}"))?],
    );
    Ok(output_value)
}
