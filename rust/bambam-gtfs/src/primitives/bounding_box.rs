//! Axis-aligned WGS84 geographic bounding box (`BoundingBox`).

use crate::primitives::Coord;

/// Axis-aligned geographic bounding box in WGS84 coordinates.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct BoundingBox {
    /// Minimum longitude (west).
    pub min_lon: f64,
    /// Minimum latitude (south).
    pub min_lat: f64,
    /// Maximum longitude (east).
    pub max_lon: f64,
    /// Maximum latitude (north).
    pub max_lat: f64,
}

impl BoundingBox {
    /// Creates a bounding box containing a single coordinate.
    #[inline]
    pub const fn from_coord(c: Coord) -> Self {
        Self {
            min_lon: c.lon,
            min_lat: c.lat,
            max_lon: c.lon,
            max_lat: c.lat,
        }
    }

    /// Computes the bounding box of an iterator of coordinates, or `None` if empty.
    pub fn from_coords(coords: impl IntoIterator<Item = Coord>) -> Option<Self> {
        let mut iter = coords.into_iter();
        let first = iter.next()?;
        let mut bbox = Self::from_coord(first);
        for c in iter {
            bbox.expand_coord(c);
        }
        Some(bbox)
    }

    /// Expands this bounding box in-place to include `c`.
    #[inline]
    pub fn expand_coord(&mut self, c: Coord) {
        self.min_lon = self.min_lon.min(c.lon);
        self.min_lat = self.min_lat.min(c.lat);
        self.max_lon = self.max_lon.max(c.lon);
        self.max_lat = self.max_lat.max(c.lat);
    }

    /// Returns the union of two bounding boxes.
    #[inline]
    pub fn union(self, other: Self) -> Self {
        Self {
            min_lon: self.min_lon.min(other.min_lon),
            min_lat: self.min_lat.min(other.min_lat),
            max_lon: self.max_lon.max(other.max_lon),
            max_lat: self.max_lat.max(other.max_lat),
        }
    }

    /// Returns the center coordinate of the bounding box.
    #[inline]
    pub fn center(&self) -> Coord {
        Coord {
            lon: (self.min_lon + self.max_lon) * 0.5,
            lat: (self.min_lat + self.max_lat) * 0.5,
        }
    }

    /// Returns true if `c` lies within or on the boundary of this bounding box.
    #[inline]
    pub fn contains(&self, c: Coord) -> bool {
        c.lon >= self.min_lon
            && c.lon <= self.max_lon
            && c.lat >= self.min_lat
            && c.lat <= self.max_lat
    }

    /// Returns true if `self` and `other` intersect (share at least one point).
    #[inline]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min_lon <= other.max_lon
            && self.max_lon >= other.min_lon
            && self.min_lat <= other.max_lat
            && self.max_lat >= other.min_lat
    }
}
