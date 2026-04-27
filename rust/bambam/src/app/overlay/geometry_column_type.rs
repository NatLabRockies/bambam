use std::collections::HashMap;

use geo::{Coord, Geometry, Point};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GeometryColumnType {
    Geometry { col: String, format: GeometryFormat },
    Xy { x: String, y: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GeometryFormat {
    Wkt,
    Wkb,
}

impl GeometryColumnType {
    pub fn new(
        x: Option<&String>,
        y: Option<&String>,
        geom: Option<&String>,
        format: Option<&GeometryFormat>,
    ) -> Result<Self, String> {
        match (x, y, geom, format) {
            (Some(x_col), Some(y_col), None, None) => Ok(Self::Xy {
                x: x_col.clone(),
                y: y_col.clone(),
            }),
            (None, None, Some(geom_col), Some(f)) => Ok(Self::Geometry { col: geom_col.clone(), format: f.clone() }),
            _ => Err(format!(
                "must either provide x and y, or geometry column name and format, found {x:?} {y:?} {geom:?} {format:?}"
            )),
        }
    }

    pub fn get_point(
        &self,
        row: &csv::StringRecord,
        lookup: &HashMap<String, usize>,
    ) -> Result<Geometry, String> {
        match self {
            GeometryColumnType::Geometry { col, format } => {
                todo!("deserialize a geometry directly from the row")
            }
            GeometryColumnType::Xy { x, y } => {
                let x_idx = lookup
                    .get(x)
                    .ok_or_else(|| format!("header missing {x} column"))?;
                let y_idx = lookup
                    .get(y)
                    .ok_or_else(|| format!("header missing {y} column"))?;
                let x_str = row
                    .get(*x_idx)
                    .ok_or_else(|| format!("row missing {x} column at index {x_idx}"))?;
                let y_str = row
                    .get(*y_idx)
                    .ok_or_else(|| format!("row missing {y} column at index {y_idx}"))?;
                let x_val = x_str
                    .parse()
                    .map_err(|e| format!("row has invalid {x} value of {x_str}: {e}"))?;
                let y_val = y_str
                    .parse()
                    .map_err(|e| format!("row has invalid {y} value of {y_str}: {e}"))?;
                let point = Point::<f64>(Coord { x: x_val, y: y_val });
                Ok(geo::Geometry::Point(point))
            }
        }
    }
}
