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
