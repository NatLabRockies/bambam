//! 2D geographic coordinate (`Coord`) in WGS84 decimal degrees.

/// A 2D geographic coordinate in WGS84 (`longitude`, `latitude` in degrees).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Coord {
    /// Longitude in decimal degrees (`x`).
    pub lon: f64,
    /// Latitude in decimal degrees (`y`).
    pub lat: f64,
}

impl Coord {
    /// Creates a new `Coord` from longitude and latitude in degrees.
    #[inline]
    pub const fn new(lon: f64, lat: f64) -> Self {
        Self { lon, lat }
    }

    /// Approximate Haversine distance in meters between two WGS84 coordinates.
    pub fn haversine_meters(self, other: Self) -> f64 {
        const EARTH_RADIUS_M: f64 = 6_371_008.8;
        let d_lat = (other.lat - self.lat).to_radians();
        let d_lon = (other.lon - self.lon).to_radians();
        let lat1 = self.lat.to_radians();
        let lat2 = other.lat.to_radians();

        let a = (d_lat * 0.5).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon * 0.5).sin().powi(2);
        let c = 2.0 * a.sqrt().atan2((1.0 - a).max(0.0).sqrt());
        EARTH_RADIUS_M * c
    }

    /// Linearly interpolates between `a` and `b` by factor `t` in `[0.0, 1.0]`.
    #[inline]
    pub fn lerp(a: Self, b: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            lon: a.lon + (b.lon - a.lon) * t,
            lat: a.lat + (b.lat - a.lat) * t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coord_haversine_and_lerp() {
        let origin = Coord::new(0.0, 0.0);
        assert!((origin.haversine_meters(origin)).abs() < 1e-9);

        // 1 degree of latitude at the equator is ~111,195 meters
        let one_deg_north = Coord::new(0.0, 1.0);
        let dist = origin.haversine_meters(one_deg_north);
        assert!(
            (dist - 111_195.0).abs() < 20.0,
            "unexpected 1-deg distance: {dist}"
        );

        let a = Coord::new(-105.0, 40.0);
        let b = Coord::new(-104.0, 41.0);
        let mid = Coord::lerp(a, b, 0.5);
        assert!((mid.lon - (-104.5)).abs() < 1e-9);
        assert!((mid.lat - 40.5).abs() < 1e-9);
        assert_eq!(Coord::lerp(a, b, -1.0), a);
        assert_eq!(Coord::lerp(a, b, 2.0), b);
    }
}
