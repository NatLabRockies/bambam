use geo::{Centroid, MapCoords};
use geo::{Geometry, Point};
use rstar::RTreeObject;
use rstar::AABB;

/// creates an envelope from a geometry using assumptions that
/// - points, linestrings, polygons can have their bboxes be their envelopes
/// - other geometry types can use their centroids
///
/// since a centroid may not exist (for example, empty geometries), the result may be None
///
/// # Arguments
///
/// * `geometry` - value to create an envelope from
///
/// # Returns
///
/// * an envelope if possible, otherwise None
pub fn get_centroid_as_envelope(geometry: &Geometry<f32>) -> Option<AABB<Point<f32>>> {
    match geometry {
        Geometry::Point(g) => Some(g.envelope()),
        Geometry::Line(g) => Some(g.envelope()),
        Geometry::LineString(g) => Some(g.envelope()),
        Geometry::Polygon(g) => Some(g.envelope()),
        Geometry::MultiPoint(g) => g.centroid().map(AABB::from_point),
        Geometry::MultiLineString(g) => g.centroid().map(AABB::from_point),
        Geometry::MultiPolygon(g) => g.centroid().map(AABB::from_point),
        Geometry::GeometryCollection(g) => g.centroid().map(AABB::from_point),
        Geometry::Rect(g) => Some(AABB::from_point(g.centroid())),
        Geometry::Triangle(g) => Some(AABB::from_point(g.centroid())),
    }
}

/// attempt to convert a geometry from 64 bit to 32 bit floating point representation.
/// this operation is destructive but warranted when working with lat/lon values and
/// scaling RAM consumption for national runs.
pub fn try_convert_f32(g: &Geometry<f64>) -> Result<Geometry<f32>, String> {
    let (min, max) = (f32::MIN as f64, f32::MAX as f64);
    g.try_map_coords(|geo::Coord { x, y }| {
        if x < min || max < x {
            Err(format!("could not express x value '{x}' as f32, exceeds range of possible values [{min}, {max}]"))
        } else if y < min || max < y {
            Err(format!("could not express y value '{y}' as f32, exceeds range of possible values [{min}, {max}]"))
        } else {
            let x32 = std::panic::catch_unwind(|| x as f32).map_err(|_| {
                format!("could not express x value '{x}' as f32")
            })?;
            let y32 = std::panic::catch_unwind(|| y as f32).map_err(|_| {
                format!("could not express y value '{x}' as f32")
            })?;
            Ok(geo::Coord { x: x32, y: y32 })
        }
    })
}
