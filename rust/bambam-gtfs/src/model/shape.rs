//! Validated `ShapePolyline` with Linear Referencing System (LRS) support for reconstructing
//! sub-segment linestrings along trip traversals.

use crate::ids::ShapeId;
use crate::model::ModelError;
use crate::primitives::{BoundingBox, Coord};
use crate::raw::RawShapePoint;
use crate::traits::{Identifiable, Spatial};

/// A single compact vertex along a [`ShapePolyline`] (`12 bytes` total: `lon`, `lat`, `cumulative_meters`).
///
/// Using `f32` (~1 cm geographic precision and ~1 mm arc-length precision over 100 km) reduces
/// 80 million continental US shape points from `~2.6 GB` down to `~960 MB`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeVertex {
    /// Longitude in WGS84 decimal degrees.
    pub lon: f32,
    /// Latitude in WGS84 decimal degrees.
    pub lat: f32,
    /// Cumulative Haversine arc length in meters from the start of the shape (`>= 0.0`).
    pub cumulative_meters: f32,
}

impl ShapeVertex {
    /// Returns this vertex's WGS84 [`Coord`].
    #[inline]
    pub fn coord(self) -> Coord {
        Coord::new(self.lon as f64, self.lat as f64)
    }
}

/// A linear position along a [`ShapePolyline`] resolved for a specific stop or point along a trip.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeLinearRef {
    /// Cumulative Haversine arc length in meters from the first vertex of the [`ShapePolyline`].
    pub arc_meters: f64,
    /// Normalized fraction in `[0.0, 1.0]` along the total length of the [`ShapePolyline`].
    pub fraction: f64,
    /// Optional feed-provided `shape_dist_traveled` value if present in `stop_times.txt` / `shapes.txt`.
    pub raw_dist_traveled: Option<f64>,
}

/// Ordered polyline from `shapes.txt` supporting Linear Referencing System (LRS) queries:
/// - projecting geographic coordinates onto the polyline,
/// - resolving `shape_dist_traveled` values into exact arc-length positions, and
/// - slicing exact sub-segment linestrings `Vec<Coord>` between any two linear references.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapePolyline {
    /// Unique shape identifier.
    pub id: ShapeId,
    /// Compact 12-byte vertices ordered by `shape_pt_sequence`.
    pub vertices: Vec<ShapeVertex>,
    /// Optional feed-provided `shape_dist_traveled` per vertex (empty when the feed omits `shape_dist_traveled`,
    /// saving 8 bytes per vertex across feeds that only provide lat/lon).
    pub vertex_raw_dists: Vec<f32>,
    /// Total Haversine length of the polyline in meters.
    pub total_meters: f64,
    /// Precomputed bounding box of the polyline.
    pub bbox: Option<BoundingBox>,
}

impl ShapePolyline {
    /// Constructs a [`ShapePolyline`] from unsorted raw shape points for a single `shape_id`.
    pub fn from_raw_points(
        id: ShapeId,
        mut points: Vec<RawShapePoint>,
    ) -> Result<Self, ModelError> {
        points.sort_by_key(|p| p.sequence);

        let has_any_raw_dist = points.iter().any(|p| p.dist_traveled.is_some());
        let mut vertices: Vec<ShapeVertex> = Vec::with_capacity(points.len());
        let mut vertex_raw_dists: Vec<f32> = if has_any_raw_dist {
            Vec::with_capacity(points.len())
        } else {
            Vec::new()
        };

        let mut cumulative_meters = 0.0_f64;
        let mut prev_raw_dist: Option<f64> = None;
        let mut bbox: Option<BoundingBox> = None;

        for (i, pt) in points.into_iter().enumerate() {
            let coord = Coord::new(pt.longitude, pt.latitude);
            if i > 0 {
                let prev_coord = vertices[i - 1].coord();
                cumulative_meters += prev_coord.haversine_meters(coord);
            }
            if let Some(d) = pt.dist_traveled {
                if let Some(prev_d) = prev_raw_dist {
                    if d < prev_d {
                        return Err(ModelError::NonMonotonicShapeDistance {
                            file_name: "shapes.txt",
                            id: id.to_string(),
                            previous: prev_d,
                            current: d,
                        });
                    }
                }
                prev_raw_dist = Some(d);
            }
            if has_any_raw_dist {
                vertex_raw_dists.push(pt.dist_traveled.map(|d| d as f32).unwrap_or(f32::NAN));
            }
            match &mut bbox {
                Some(b) => b.expand_coord(coord),
                None => bbox = Some(BoundingBox::from_coord(coord)),
            }
            vertices.push(ShapeVertex {
                lon: pt.longitude as f32,
                lat: pt.latitude as f32,
                cumulative_meters: cumulative_meters as f32,
            });
        }

        Ok(Self {
            id,
            vertices,
            vertex_raw_dists,
            total_meters: cumulative_meters,
            bbox,
        })
    }

    /// Creates a [`ShapeLinearRef`] from an arc-length distance in meters along this polyline.
    pub fn linear_ref_at_meters(&self, meters: f64, raw_dist: Option<f64>) -> ShapeLinearRef {
        let clamped = meters.clamp(0.0, self.total_meters);
        let fraction = if self.total_meters > 0.0 {
            (clamped / self.total_meters).clamp(0.0, 1.0)
        } else {
            0.0
        };
        ShapeLinearRef {
            arc_meters: clamped,
            fraction,
            raw_dist_traveled: raw_dist,
        }
    }

    /// Resolves a feed-provided `shape_dist_traveled` value into a [`ShapeLinearRef`].
    ///
    /// If the vertices in `shapes.txt` also carry `shape_dist_traveled`, this interpolates
    /// the exact Haversine meter offset matching `raw_dist`. Otherwise, if `raw_dist` is within
    /// `[0, total_meters]`, it is treated directly as meters.
    pub fn resolve_raw_dist(&self, raw_dist: f64) -> Option<ShapeLinearRef> {
        if self.vertices.is_empty() {
            return None;
        }

        if !self.vertex_raw_dists.is_empty() {
            let first_raw = self
                .vertex_raw_dists
                .first()
                .copied()
                .filter(|d| !d.is_nan())
                .unwrap_or(0.0) as f64;
            if raw_dist <= first_raw {
                return Some(self.linear_ref_at_meters(0.0, Some(raw_dist)));
            }
            for i in 0..self.vertices.len().saturating_sub(1) {
                let d0 = self.vertex_raw_dists[i] as f64;
                let d1 = self.vertex_raw_dists[i + 1] as f64;
                if !d0.is_nan() && !d1.is_nan() && raw_dist >= d0 && raw_dist <= d1 {
                    let span = d1 - d0;
                    let t = if span > 0.0 {
                        (raw_dist - d0) / span
                    } else {
                        0.0
                    };
                    let v0_m = self.vertices[i].cumulative_meters as f64;
                    let v1_m = self.vertices[i + 1].cumulative_meters as f64;
                    let m = v0_m + (v1_m - v0_m) * t;
                    return Some(self.linear_ref_at_meters(m, Some(raw_dist)));
                }
            }
            return Some(self.linear_ref_at_meters(self.total_meters, Some(raw_dist)));
        }

        Some(self.linear_ref_at_meters(raw_dist, Some(raw_dist)))
    }

    /// Projects a geographic coordinate `target` onto the polyline, returning the closest
    /// [`ShapeLinearRef`] with `arc_meters >= min_arc_meters` (to preserve monotonic stop order along loops).
    pub fn project_coord_monotonic(
        &self,
        target: Coord,
        min_arc_meters: f64,
    ) -> Option<ShapeLinearRef> {
        if self.vertices.is_empty() {
            return None;
        }
        if self.vertices.len() == 1 {
            let raw = self
                .vertex_raw_dists
                .first()
                .copied()
                .filter(|d| !d.is_nan())
                .map(|d| d as f64);
            return Some(self.linear_ref_at_meters(0.0, raw));
        }

        let mut best_dist_sq = f64::INFINITY;
        let mut best_arc_meters = min_arc_meters.clamp(0.0, self.total_meters);

        let cos_lat = target.lat.to_radians().cos().max(1e-6);

        for window in self.vertices.windows(2) {
            let (v0, v1) = (window[0], window[1]);
            let v0_m = v0.cumulative_meters as f64;
            let v1_m = v1.cumulative_meters as f64;
            if v1_m < min_arc_meters {
                continue;
            }

            let ax = (v0.lon as f64 - target.lon) * cos_lat;
            let ay = v0.lat as f64 - target.lat;
            let bx = (v1.lon as f64 - target.lon) * cos_lat;
            let by = v1.lat as f64 - target.lat;

            let dx = bx - ax;
            let dy = by - ay;
            let seg_len_sq = dx * dx + dy * dy;

            let mut t = if seg_len_sq > 0.0 {
                (-(ax * dx + ay * dy) / seg_len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let seg_meters = v1_m - v0_m;
            let mut candidate_meters = v0_m + t * seg_meters;
            if candidate_meters < min_arc_meters {
                candidate_meters = min_arc_meters;
                t = if seg_meters > 0.0 {
                    ((candidate_meters - v0_m) / seg_meters).clamp(0.0, 1.0)
                } else {
                    0.0
                };
            }

            let px = ax + t * dx;
            let py = ay + t * dy;
            let dist_sq = px * px + py * py;

            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                best_arc_meters = candidate_meters;
            }
        }

        Some(self.linear_ref_at_meters(best_arc_meters, None))
    }

    /// Interpolates the exact [`Coord`] at `arc_meters` along this polyline.
    pub fn coord_at_meters(&self, arc_meters: f64) -> Option<Coord> {
        if self.vertices.is_empty() {
            return None;
        }
        if arc_meters <= 0.0 || self.vertices.len() == 1 {
            return Some(self.vertices[0].coord());
        }
        if arc_meters >= self.total_meters {
            return Some(self.vertices.last()?.coord());
        }

        let target_f32 = arc_meters as f32;
        let idx = self
            .vertices
            .partition_point(|v| v.cumulative_meters < target_f32);
        if idx == 0 {
            return Some(self.vertices[0].coord());
        }
        let v0 = self.vertices[idx - 1];
        let v1 = self.vertices[idx];
        let span = (v1.cumulative_meters - v0.cumulative_meters) as f64;
        let t = if span > 0.0 {
            (arc_meters - v0.cumulative_meters as f64) / span
        } else {
            0.0
        };
        Some(Coord::lerp(v0.coord(), v1.coord(), t))
    }

    /// Extracts the sub-segment linestring (`Vec<Coord>`) between `start_meters` and `end_meters`,
    /// including exact interpolated start and end coordinates and all intermediate shape vertices.
    pub fn slice_linestring(&self, start_meters: f64, end_meters: f64) -> Vec<Coord> {
        if self.vertices.is_empty() {
            return Vec::new();
        }
        let s = start_meters.clamp(0.0, self.total_meters);
        let e = end_meters.clamp(s, self.total_meters);

        let mut coords = Vec::new();
        if let Some(start_coord) = self.coord_at_meters(s) {
            coords.push(start_coord);
        }

        for v in &self.vertices {
            let vm = v.cumulative_meters as f64;
            if vm > s && vm < e {
                let vc = v.coord();
                if coords.last() != Some(&vc) {
                    coords.push(vc);
                }
            }
        }

        if let Some(end_coord) = self.coord_at_meters(e) {
            if coords.last() != Some(&end_coord) {
                coords.push(end_coord);
            }
        }

        coords
    }
}

impl Identifiable for ShapePolyline {
    type Id = ShapeId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

impl Spatial for ShapePolyline {
    #[inline]
    fn representative_coord(&self) -> Option<Coord> {
        self.coord_at_meters(self.total_meters * 0.5)
    }

    #[inline]
    fn bounding_box(&self) -> Option<BoundingBox> {
        self.bbox
    }
}
