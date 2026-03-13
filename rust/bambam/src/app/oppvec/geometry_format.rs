use csv::StringRecord;
use geo::{Geometry, MapCoords};
use geozero::{wkt::Wkt as WktReader, ToGeo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum GeometryFormat {
    WktColumn { column_name: String },
    XYColumns { x_column: String, y_column: String },
}

impl GeometryFormat {
    pub fn new_wkt_format(column_name: String) -> GeometryFormat {
        GeometryFormat::WktColumn { column_name }
    }

    pub fn new_xy_format(x_column: String, y_column: String) -> GeometryFormat {
        GeometryFormat::XYColumns { x_column, y_column }
    }

    /// validates the provided column parameters and creates the appropriate [`GeometryFormat`] instance
    /// as a result.
    pub fn new(
        geometry_column: Option<&String>,
        x_column: Option<&String>,
        y_column: Option<&String>,
    ) -> Result<GeometryFormat, String> {
        match (geometry_column, x_column, y_column) {
            (Some(col), None, None) => Ok(Self::new_wkt_format(col.clone())),
            (None, Some(x), Some(y)) => Ok(Self::new_xy_format(x.clone(), y.clone())),
            _ => Err(String::from(
                "specify only a geometry_column or provide x and y columns, not both",
            )),
        }
    }
}

impl std::fmt::Display for GeometryFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeometryFormat::WktColumn { column_name } => {
                write!(f, "{column_name}")
            }
            GeometryFormat::XYColumns { x_column, y_column } => {
                write!(f, "{x_column},{y_column}")
            }
        }
    }
}

impl GeometryFormat {
    pub fn get_geometry(
        &self,
        row: &StringRecord,
        column_index_lookup: &HashMap<String, usize>,
    ) -> Result<geo::Geometry<f32>, String> {
        match self {
            GeometryFormat::WktColumn { column_name } => {
                let idx = column_index_lookup
                    .get(column_name)
                    .ok_or_else(|| format!("file does not contain column '{column_name}'"))?;
                let value = row.get(*idx).ok_or_else(|| format!("internal error: column index lookup has col '{column_name}' at idx '{idx}' which is not found in the lookup"))?;
                let g = WktReader(value)
                    .to_geo()
                    .map(|geom| {
                        geom.map_coords(|c| geo::Coord {
                            x: c.x as f32,
                            y: c.y as f32,
                        })
                    })
                    .map_err(|e| {
                        format!("failure reading geometry at column '{column_name}': {e}")
                    })?;
                Ok(g)
                // match g {
                //     Geometry::Point(point) => Ok(point),
                //     _ => Err(format!("geometry must be point, found '{}'", value)),
                // }
            }
            GeometryFormat::XYColumns { x_column, y_column } => {
                let x_idx = column_index_lookup
                    .get(x_column)
                    .ok_or_else(|| format!("file does not contain column '{x_column}'"))?;
                let y_idx = column_index_lookup
                    .get(y_column)
                    .ok_or_else(|| format!("file does not contain column '{y_column}'"))?;
                let x_str = row.get(*x_idx).ok_or_else(|| format!("internal error: column index lookup has col '{x_column}' at idx '{x_idx}' which is not found in the lookup"))?;
                let y_str = row.get(*y_idx).ok_or_else(|| format!("internal error: column index lookup has col '{y_column}' at idx '{y_idx}' which is not found in the lookup"))?;
                let x = x_str
                    .parse::<f32>()
                    .map_err(|e| format!("failure reading number in column '{x_column}': {e}"))?;
                let y = y_str
                    .parse::<f32>()
                    .map_err(|e| format!("failure reading number in column '{y_column}': {e}"))?;
                let point = geo::Point::new(x, y);
                Ok(geo::Geometry::Point(point))
            }
        }
    }
}
