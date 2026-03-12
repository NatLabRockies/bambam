use geozero::ToWkt;
use serde_json::json;

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
        json![geometry
            .to_wkt()
            .map_err(|e| format!("failure serializing geometry as WKT: {e}"))?],
    );
    Ok(output_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_grid_row_polygon() {
        use geo::{coord, polygon};
        let template = serde_json::json!({ "origin_weight": 1.0 });
        let poly = polygon![
            (x: 0.0f64, y: 0.0f64),
            (x: 1.0f64, y: 0.0f64),
            (x: 1.0f64, y: 1.0f64),
            (x: 0.0f64, y: 1.0f64),
            (x: 0.0f64, y: 0.0f64),
        ];
        let geom = geo::Geometry::Polygon(poly);
        let result = create_grid_row("h3-cell-1".to_string(), 0.5, 0.5, &geom, &template)
            .expect("should succeed creating grid row");
        let geom_val = result["geometry"]
            .as_str()
            .expect("geometry should be a string");
        assert!(
            geom_val.starts_with("POLYGON"),
            "geometry WKT should start with POLYGON, got: {geom_val}"
        );
        assert_eq!(result["origin_x"], serde_json::json!(0.5));
        assert_eq!(result["origin_y"], serde_json::json!(0.5));
        assert_eq!(result["grid_id"], serde_json::json!("h3-cell-1"));
        // suppress unused import warnings in non-test builds
        let _ = coord! { x: 0.0, y: 0.0 };
    }

    #[test]
    fn test_create_grid_row_invalid_template() {
        use geo::polygon;
        let template = serde_json::json!("not-an-object");
        let geom = geo::Geometry::Polygon(polygon![
            (x: 0.0f64, y: 0.0f64),
            (x: 1.0f64, y: 0.0f64),
            (x: 1.0f64, y: 1.0f64),
            (x: 0.0f64, y: 0.0f64),
        ]);
        let result = create_grid_row("cell".to_string(), 0.0, 0.0, &geom, &template);
        assert!(result.is_err(), "should fail on non-object template");
    }
}
