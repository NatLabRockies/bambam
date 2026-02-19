use geo::{
    line_string, Centroid, Coord, CoordFloat, Destination, GeoFloat, Geometry, Haversine,
    KNearestConcaveHull, Length, LineString, Point, Polygon, Scale,
};
use itertools::Itertools;
use num_traits::FromPrimitive;

pub trait Buffer<F: CoordFloat + FromPrimitive> {
    /// buffer a geometry up to some distance. GEOS uses a fancier method to determine
    /// the resolution, but here, we simply assign points at 15 meter intervals along
    /// the curve since that is our typical tolerance for spatial errors. or, if that
    /// produces fewer than six points, we use six, since that at least gives us a hexagon.
    ///
    /// for GEOS' approach, see [this page](https://github.com/libgeos/geos/blob/main/src/operation/buffer/OffsetSegmentGenerator.cpp#L78)
    /// where `filletAngleQuantum` is calculated and then used to set the `maxCurveSegmentError`
    /// before building buffer segments.
    fn buffer(&self, size: uom::si::f64::Length) -> Result<geo::Geometry<F>, String>;
}
mod consts {
    // pub const MIN_RES: usize = 6;
    pub const MAX_RES: usize = 12;
}

impl Buffer<f32> for Point<f32> {
    fn buffer(&self, size: uom::si::f64::Length) -> Result<geo::Geometry<f32>, String> {
        let x = self.x() as f64;
        let y = self.y() as f64;
        let point: Point<f64> = geo::Point(geo::Coord::from((x, y)));
        let dist_meters = size.get::<uom::si::length::meter>();
        let resolution = consts::MAX_RES;
        let buffer = create_buffer(&point, dist_meters, resolution);
        let buf32: Polygon<f32> = geo::Polygon::new(
            geo::LineString::from(
                buffer
                    .exterior()
                    .into_iter()
                    .map(|c| geo::Coord::from((c.x as f32, c.y as f32)))
                    .collect_vec(),
            ),
            vec![],
        );
        Ok(geo::Geometry::Polygon(buf32))
    }
}

/// the rust geo library does not support buffering.
///
/// this solution comes from a [discussion thread](https://github.com/georust/geo/issues/641#issuecomment-2236078216)
/// on the github.com/georust/geo repo which proposes this way to buffer a point.
///
/// # Arguments
///
/// * `point` - a WGS84 point with x,y ordering
/// * `radius` - buffer size, in meters
/// * `resolution` - number of evenly-space points to place along the new buffer cirumference.
///  more points make a better approximation of a circle.
///
/// * Returns
///
/// A circlular Polygon
fn create_buffer(point: &Point<f64>, radius: f64, resolution: usize) -> Polygon<f64> {
    let mut coordinates: Vec<(f64, f64)> = Vec::with_capacity(resolution + 1);

    for i in 0..=resolution {
        let angle = i as f64 * 360.0 / resolution as f64;
        let dest = Haversine.destination(*point, angle, radius);
        coordinates.push((dest.x(), dest.y()));
    }
    let first = Haversine.destination(*point, 0.0, radius);
    coordinates.push((first.x(), first.y())); // close the circle!

    let line_string = LineString::from(coordinates);
    Polygon::new(line_string, vec![])
}

/// scales the exterior points of a geometry by some distance.
/// the distance should be in the unit that matches the output of the parameterized Distance
/// trait. for example, using [`geo::Haversine`] expects points in WGS84 lat/lon degrees and
/// outputs distances in meters, so, distance should be provided in meters.
///
/// distances may be negative.
///
/// # Arguments
///
/// * `geometry` - the geometry to scale, must be a Polygon or MultiPolygon
/// * `distance` - distance to extend all sides of the extent of the geometry
///
/// # Returns
/// The scaled geometry, a polygon
pub fn scale_exterior<T>(geometry: &Geometry<T>, distance: T) -> Result<Geometry<T>, String>
where
    T: FromPrimitive + GeoFloat,
{
    // find all exterior coordinates
    let exterior: Vec<Coord<T>> = match geometry {
        Geometry::Polygon(polygon) => {
            let exterior = polygon.exterior().coords().cloned().collect_vec();
            Ok(exterior)
        }
        Geometry::MultiPolygon(multi_polygon) => {
            let exteriors = multi_polygon
                .iter()
                .flat_map(|p| p.exterior())
                .cloned()
                .collect_vec();
            Ok(exteriors)
        }
        _ => Err(String::from(
            "geometry must be POLYGON or MULTIPOLYGON in order to scale the exterior",
        )),
    }?;
    let hull: Polygon<T> = exterior.k_nearest_concave_hull(3);
    let centroid = hull.centroid().ok_or_else(|| {
        format!(
            "unable to get centroid of geometry's exterior hull (made of {} points)",
            hull.exterior().0.len()
        )
    })?;
    // find the maximum "radius" of the hull polygon
    let mut max_radius: T = T::zero();
    for p in hull.exterior().points() {
        let line: LineString<T> = line_string![centroid.0, p.0];
        let length = Haversine.length(&line);
        if max_radius < length {
            max_radius = length;
        }
    }

    // scale the hull proportional to the max radius extended by the distance parameter
    let scale_factor = (max_radius + distance) / max_radius;
    if scale_factor < T::zero() {
        return Err(format!(
            "distance {distance:#?} leads to wrapping over zero as max radius is {max_radius:#?}"
        ));
    }
    let scaled = hull.scale(scale_factor);
    Ok(geo::Geometry::Polygon(scaled))
}
