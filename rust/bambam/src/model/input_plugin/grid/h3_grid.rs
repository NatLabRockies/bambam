use super::grid_ops;
use geo::Centroid;
use geozero::ToWkt;
use h3o::geom::{ContainmentMode, SolventBuilder, TilerBuilder};
use itertools::Itertools;

pub fn from_polygon_extent(
    extent: &geo::Polygon,
    template: &serde_json::Value,
    resolution: &h3o::Resolution,
) -> Result<Vec<serde_json::Value>, String> {
    let mut tiler = h3o::geom::TilerBuilder::new(*resolution)
        .containment_mode(ContainmentMode::IntersectsBoundary)
        // .with_polygon(extent.clone())
        .build();
    tiler
        .add(extent.clone())
        .map_err(|e| format!("failure adding extent to h3 tiler: {e}"))?;

    let cells = tiler.into_coverage().collect_vec();

    cells
        .into_iter()
        .map(|cell| {
            let line: geo::LineString = cell.boundary().into();
            let polygon = geo::Polygon::new(line, vec![]);
            let centroid = polygon.centroid().ok_or_else(|| {
                format!(
                    "unable to retrieve centroid of polygon: {}",
                    geo::Geometry::from(polygon.clone())
                        .to_wkt()
                        .unwrap_or_default()
                )
            })?;
            let row = grid_ops::create_grid_row(
                cell.to_string(),
                centroid.x(),
                centroid.y(),
                &geo::Geometry::Polygon(polygon),
                template,
            )?;
            Ok(row)
        })
        .collect::<Result<Vec<_>, _>>()
}
