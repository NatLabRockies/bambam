//! Polygon and MultiPolygon geometry (`RawFlexGeometry`) and exact ray-casting point-in-polygon checks.

use crate::primitives::Coord;

/// Validated polygon or multipolygon rings parsed from `locations.geojson` (GTFS-Flex).
///
/// Unlike an untagged enum wrapping `geojson::Geometry` in both variants, this enum
/// holds the strongly-typed coordinate rings directly and serializes/deserializes cleanly.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum RawFlexGeometry {
    /// A single polygon (exterior ring followed by zero or more interior hole rings).
    Polygon(Vec<Vec<Coord>>),
    /// A collection of polygons.
    MultiPolygon(Vec<Vec<Vec<Coord>>>),
}

/// Type alias for `RawFlexGeometry`.
pub type RawFlexLocationGeometry = RawFlexGeometry;

impl RawFlexGeometry {
    /// Returns `true` if `coord` lies inside or on the boundary of this Polygon or MultiPolygon
    /// (accounting for interior hole rings via odd-even ray casting).
    pub fn contains_coord(&self, coord: Coord) -> bool {
        match self {
            RawFlexGeometry::Polygon(rings) => polygon_rings_contain_coord(rings, coord),
            RawFlexGeometry::MultiPolygon(polys) => polys
                .iter()
                .any(|rings| polygon_rings_contain_coord(rings, coord)),
        }
    }
}

fn polygon_rings_contain_coord(rings: &[Vec<Coord>], coord: Coord) -> bool {
    let Some((exterior, holes)) = rings.split_first() else {
        return false;
    };
    if !ring_contains_coord(exterior, coord) {
        return false;
    }
    for hole in holes {
        if ring_contains_coord_strict_interior(hole, coord) {
            return false;
        }
    }
    true
}

fn point_on_segment(p: Coord, a: Coord, b: Coord) -> bool {
    const EPS: f64 = 1e-11;
    let min_x = a.lon.min(b.lon) - EPS;
    let max_x = a.lon.max(b.lon) + EPS;
    let min_y = a.lat.min(b.lat) - EPS;
    let max_y = a.lat.max(b.lat) + EPS;
    if p.lon < min_x || p.lon > max_x || p.lat < min_y || p.lat > max_y {
        return false;
    }
    let cross = (b.lon - a.lon) * (p.lat - a.lat) - (b.lat - a.lat) * (p.lon - a.lon);
    let seg_len = ((b.lon - a.lon).hypot(b.lat - a.lat)).max(1.0);
    (cross.abs() / seg_len) <= EPS
}

fn ring_ray_cast(ring: &[Coord], coord: Coord) -> (bool, bool) {
    let n = ring.len();
    if n < 3 {
        return (false, false);
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let pi = ring[i];
        let pj = ring[j];
        if point_on_segment(coord, pj, pi) {
            return (true, true);
        }
        if (pi.lat > coord.lat) != (pj.lat > coord.lat) {
            let x_intersect = (pj.lon - pi.lon) * (coord.lat - pi.lat) / (pj.lat - pi.lat) + pi.lon;
            if coord.lon < x_intersect {
                inside = !inside;
            }
        }
        j = i;
    }
    (inside, false)
}

/// Returns `true` if `coord` is inside or on the boundary of `ring`.
fn ring_contains_coord(ring: &[Coord], coord: Coord) -> bool {
    let (inside, on_boundary) = ring_ray_cast(ring, coord);
    inside || on_boundary
}

/// Returns `true` if `coord` is strictly inside `ring` (used for interior holes so points on a hole boundary remain in the polygon).
fn ring_contains_coord_strict_interior(ring: &[Coord], coord: Coord) -> bool {
    let (inside, on_boundary) = ring_ray_cast(ring, coord);
    inside && !on_boundary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_flex_geometry_contains_coord_convex_concave_holes_and_multipolygon() {
        // 1. Concave L-shaped polygon where bounding-box center (0.75, 0.75) is outside the L
        let l_ring = vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 0.4),
            Coord::new(0.4, 0.4),
            Coord::new(0.4, 1.0),
            Coord::new(0.0, 1.0),
            Coord::new(0.0, 0.0),
        ];
        let l_poly = RawFlexGeometry::Polygon(vec![l_ring]);
        assert!(l_poly.contains_coord(Coord::new(0.8, 0.2))); // inside bottom arm
        assert!(l_poly.contains_coord(Coord::new(0.2, 0.8))); // inside left arm
        assert!(l_poly.contains_coord(Coord::new(0.0, 0.0))); // exact vertex
        assert!(l_poly.contains_coord(Coord::new(0.5, 0.0))); // exact horizontal edge
        assert!(l_poly.contains_coord(Coord::new(0.0, 0.5))); // exact vertical edge
        assert!(!l_poly.contains_coord(Coord::new(0.8, 0.8))); // in L cutout
        assert!(!l_poly.contains_coord(Coord::new(2.0, 2.0))); // completely outside

        // 2. Polygon with an interior hole ring
        let outer = vec![
            Coord::new(0.0, 0.0),
            Coord::new(10.0, 0.0),
            Coord::new(10.0, 10.0),
            Coord::new(0.0, 10.0),
            Coord::new(0.0, 0.0),
        ];
        let hole = vec![
            Coord::new(3.0, 3.0),
            Coord::new(7.0, 3.0),
            Coord::new(7.0, 7.0),
            Coord::new(3.0, 7.0),
            Coord::new(3.0, 3.0),
        ];
        let donut = RawFlexGeometry::Polygon(vec![outer, hole]);
        assert!(donut.contains_coord(Coord::new(1.0, 1.0))); // between outer and hole
        assert!(!donut.contains_coord(Coord::new(5.0, 5.0))); // strictly inside hole -> false
        assert!(donut.contains_coord(Coord::new(3.0, 5.0))); // on hole boundary -> true

        // 3. MultiPolygon with two disjoint squares
        let sq1 = vec![vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 1.0),
            Coord::new(0.0, 1.0),
            Coord::new(0.0, 0.0),
        ]];
        let sq2 = vec![vec![
            Coord::new(5.0, 5.0),
            Coord::new(6.0, 5.0),
            Coord::new(6.0, 6.0),
            Coord::new(5.0, 6.0),
            Coord::new(5.0, 5.0),
        ]];
        let multi = RawFlexGeometry::MultiPolygon(vec![sq1, sq2]);
        assert!(multi.contains_coord(Coord::new(0.5, 0.5)));
        assert!(multi.contains_coord(Coord::new(5.5, 5.5)));
        assert!(!multi.contains_coord(Coord::new(3.0, 3.0)));
    }
}
