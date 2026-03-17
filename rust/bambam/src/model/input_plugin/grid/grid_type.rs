use std::fmt::Display;

use geo::{Centroid, Geometry};
// use h3o::geom::{PolyfillConfig, ToCells, ToGeo};
use serde::{Deserialize, Serialize};

use super::h3_grid;

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum GridType {
    H3 { resolution: h3o::Resolution },
}

impl Display for GridType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GridType::H3 { resolution } => write!(f, "h3({resolution})"),
        }
    }
}

impl GridType {
    pub fn create_grid(
        &self,
        extent: &geo::Geometry,
        template: &serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        match self {
            GridType::H3 { resolution } => match extent {
                geo::Geometry::Polygon(polygon) => {
                    h3_grid::from_polygon_extent(polygon, template, resolution)
                }
                geo::Geometry::MultiPolygon(mp) => {
                    log::info!(
                        "input MULTIPOLYGON has {} polygons to generate grid",
                        mp.0.len()
                    );
                    let nested = mp
                        .into_iter()
                        .map(|p| self.create_grid(&geo::Geometry::Polygon(p.clone()), template))
                        .collect::<Result<Vec<_>, _>>()?;
                    let result = nested.into_iter().flatten().collect::<Vec<_>>();
                    Ok(result)
                }
                geo::Geometry::GeometryCollection(gc) => {
                    log::info!(
                        "input GEOMETRYCOLLECTION has {} geometries to generate grid",
                        gc.0.len()
                    );
                    let nested = gc
                        .into_iter()
                        .map(|g| self.create_grid(g, template))
                        .collect::<Result<Vec<_>, _>>()?;
                    let result = nested.into_iter().flatten().collect::<Vec<_>>();
                    Ok(result)
                }
                _ => Err(String::from(
                    "unsupported extent geometry type, must be polygonal",
                )),
            },
        }
    }
}
