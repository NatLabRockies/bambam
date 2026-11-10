use std::{collections::HashSet, io::BufRead, path::Path};

use geo::{Bearing, BoundingRect, Coord, Distance, Euclidean, Haversine, LineString, Point, Rect};
use rayon::prelude::*;
use routee_compass_core::{model::unit::SpeedUnit, util::fs::read_utils};
use rstar::{RTree, RTreeObject, AABB};
use serde::{Deserialize, Serialize};

use crate::{
    config::{ConflationConfig, TomTomNetworkConfig},
    error::CongestionError,
    network::{open_buffered_reader, parse_wkt_linestring, LoadedOpenNetwork, OpenNetworkEdge},
};

/// Minimum meters per degree of latitude (~110.574 km).
const METERS_PER_DEGREE_LAT: f64 = 110_574.27;
/// Equatorial meters per degree of longitude (~111.319 km).
const METERS_PER_DEGREE_LON_EQ: f64 = 111_319.49;

/// Computes the true geodesic bearing in degrees `[0.0, 360.0)` from `p1` to `p2`,
/// where coordinates are WGS84 `(lon, lat)`.
#[inline]
pub fn geodesic_bearing_deg(p1: Coord<f64>, p2: Coord<f64>) -> f64 {
    let bearing = Haversine.bearing(Point::from(p1), Point::from(p2));
    bearing.rem_euclid(360.0)
}

/// Computes the length-weighted circular mean heading in degrees `[0.0, 360.0)`
/// of a WGS84 `(lon, lat)` polyline.
///
/// Fixes two issues in the legacy Python `calculate_bearing` / `calculate_mean_heading`:
/// 1. Properly treats `(x, y)` as `(lon, lat)` rather than `(lat, lon)`.
/// 2. Weights each sub-segment's unit heading vector $(\sin\theta_k, \cos\theta_k)$ by its
///    geodesic length $\ell_k$ so finely discretized curves do not dominate long straight segments.
pub fn length_weighted_circular_mean_heading_deg(linestring: &LineString<f64>) -> Option<f64> {
    let coords = &linestring.0;
    if coords.len() < 2 {
        return None;
    }

    let mut sum_sin = 0.0;
    let mut sum_cos = 0.0;
    let mut total_len = 0.0;

    for window in coords.windows(2) {
        let p1 = window[0];
        let p2 = window[1];
        let len_m = Haversine.distance(Point::from(p1), Point::from(p2));
        if len_m <= 1e-6 {
            continue;
        }
        let bearing_rad = geodesic_bearing_deg(p1, p2).to_radians();
        sum_sin += len_m * bearing_rad.sin();
        sum_cos += len_m * bearing_rad.cos();
        total_len += len_m;
    }

    if total_len <= 1e-6 {
        let p1 = coords[0];
        let p2 = coords[coords.len() - 1];
        if (p1.x - p2.x).abs() <= 1e-12 && (p1.y - p2.y).abs() <= 1e-12 {
            return None;
        }
        return Some(geodesic_bearing_deg(p1, p2));
    }

    let mean_deg = sum_sin.atan2(sum_cos).to_degrees().rem_euclid(360.0);
    Some(mean_deg)
}

/// Computes the wrap-around absolute angular difference in degrees `[0.0, 180.0]`
/// between two headings `h1` and `h2` (in degrees).
///
/// Fixes the `0° / 360°` boundary bug in `abs(h1 - h2) <= 45` (e.g., `2°` vs `358°` -> `4°`).
#[inline]
pub fn circular_heading_diff_deg(h1: f64, h2: f64) -> f64 {
    ((h1 - h2 + 180.0).rem_euclid(360.0) - 180.0).abs()
}

/// Expands a WGS84 bounding box `Rect<f64>` by `distance_meters` in all directions.
pub fn expand_bbox_by_meters(bbox: Rect<f64>, distance_meters: f64) -> Rect<f64> {
    let max_abs_lat = bbox.min().y.abs().max(bbox.max().y.abs()).clamp(0.0, 89.5);
    let cos_lat = max_abs_lat.to_radians().cos().max(0.01);
    let d_lat = (distance_meters / METERS_PER_DEGREE_LAT) * 1.05;
    let d_lon = (distance_meters / (METERS_PER_DEGREE_LON_EQ * cos_lat)) * 1.05;

    Rect::new(
        Coord {
            x: bbox.min().x - d_lon,
            y: bbox.min().y - d_lat,
        },
        Coord {
            x: bbox.max().x + d_lon,
            y: bbox.max().y + d_lat,
        },
    )
}

/// Local tangent-plane projection centered at `(lon0, lat0)` converting WGS84 `(lon, lat)`
/// degrees into planar meters `(x_m, y_m)`.
#[derive(Debug, Clone, Copy)]
pub struct LocalMeterProjector {
    lon0: f64,
    lat0: f64,
    meters_per_deg_lon: f64,
}

impl LocalMeterProjector {
    #[inline]
    pub fn new(lon0: f64, lat0: f64) -> Self {
        let cos_lat = lat0.to_radians().cos().abs().max(0.01);
        Self {
            lon0,
            lat0,
            meters_per_deg_lon: METERS_PER_DEGREE_LON_EQ * cos_lat,
        }
    }

    #[inline]
    pub fn project_coord(&self, c: Coord<f64>) -> Coord<f64> {
        Coord {
            x: (c.x - self.lon0) * self.meters_per_deg_lon,
            y: (c.y - self.lat0) * METERS_PER_DEGREE_LAT,
        }
    }

    pub fn project_linestring(&self, ls: &LineString<f64>) -> LineString<f64> {
        LineString(ls.0.iter().map(|c| self.project_coord(*c)).collect())
    }
}

/// Computes the length (in meters) of `candidate` that falls within `buffer_radius_m`
/// of `target` in local planar meters.
///
/// Uses fast endpoint distance checks when a sub-segment is strictly inside or outside
/// the corridor, and subdivides straddling sub-segments into 8 intervals.
pub fn compute_corridor_overlap_length_meters(
    target_m: &LineString<f64>,
    candidate_m: &LineString<f64>,
    buffer_radius_m: f64,
) -> f64 {
    if target_m.0.len() < 2 || candidate_m.0.len() < 2 {
        return 0.0;
    }

    let mut overlap_len = 0.0;
    for seg_coords in candidate_m.0.windows(2) {
        let a = seg_coords[0];
        let b = seg_coords[1];
        let seg_len = Euclidean.distance(a, b);
        if seg_len <= 1e-6 {
            continue;
        }

        let da = point_to_linestring_distance_m(a, target_m);
        let db = point_to_linestring_distance_m(b, target_m);

        // If the entire segment is too far away even at its closest possible point, skip
        if da > buffer_radius_m + seg_len && db > buffer_radius_m + seg_len {
            continue;
        }

        // If both endpoints are well inside the corridor and the segment is short, count full length
        if da <= buffer_radius_m && db <= buffer_radius_m && seg_len <= buffer_radius_m {
            overlap_len += seg_len;
            continue;
        }

        // Subdivide segment into 8 midpoint samples for accurate straddling clip length
        const SUBDIVISIONS: usize = 8;
        let step_len = seg_len / (SUBDIVISIONS as f64);
        for k in 0..SUBDIVISIONS {
            let t = (k as f64 + 0.5) / (SUBDIVISIONS as f64);
            let mid = Coord {
                x: a.x + t * (b.x - a.x),
                y: a.y + t * (b.y - a.y),
            };
            if point_to_linestring_distance_m(mid, target_m) <= buffer_radius_m {
                overlap_len += step_len;
            }
        }
    }

    overlap_len
}

#[inline]
pub fn point_to_segment_distance_m(pt: Coord<f64>, a: Coord<f64>, b: Coord<f64>) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    let proj = if len_sq <= 1e-18 {
        a
    } else {
        let t = (((pt.x - a.x) * dx + (pt.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
        Coord {
            x: a.x + t * dx,
            y: a.y + t * dy,
        }
    };
    let ex = pt.x - proj.x;
    let ey = pt.y - proj.y;
    (ex * ex + ey * ey).sqrt()
}

#[inline]
pub fn point_to_linestring_distance_m(pt: Coord<f64>, ls: &LineString<f64>) -> f64 {
    let mut min_dist = f64::INFINITY;
    for w in ls.0.windows(2) {
        let d = point_to_segment_distance_m(pt, w[0], w[1]);
        if d < min_dist {
            min_dist = d;
        }
    }
    min_dist
}

/// A directed TomTom edge indexed in an `rstar::RTree` with its 6 time-of-day speeds (in mph).
#[derive(Debug, Clone)]
pub struct TomTomEdgeEntry {
    pub tomtom_edge_id: usize,
    pub mean_heading_deg: f64,
    /// Observed speeds in mph for `[wd-am, wd-af, wd-pm, we-am, we-af, we-pm]`.
    pub speeds_mph: [Option<f64>; 6],
    pub geometry: LineString<f64>,
    pub bbox: AABB<[f64; 2]>,
}

impl RTreeObject for TomTomEdgeEntry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.bbox
    }
}

/// Result of conflating a single Open Network edge with nearby TomTom edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflatedEdgeSpeeds {
    pub edge_id: usize,
    pub total_overlap_meters: f64,
    pub matched_tomtom_count: usize,
    /// Conflated (and optionally outlier-capped) speeds in mph for
    /// `[wd-am, wd-af, wd-pm, we-am, we-af, we-pm]`.
    pub speeds_mph: [Option<f64>; 6],
}

#[derive(Deserialize)]
struct SpeedProfileCsvRow {
    speed_profile_id: usize,
    monday_trace_id: Option<usize>,
    tuesday_trace_id: Option<usize>,
    wednesday_trace_id: Option<usize>,
    thursday_trace_id: Option<usize>,
    friday_trace_id: Option<usize>,
    saturday_trace_id: Option<usize>,
    sunday_trace_id: Option<usize>,
}

/// Streams the TomTom MultiNet-R dataset, filters to `study_bbox` (expanded by `buffer_distance_meters`),
/// converts speeds to `mph`, and builds an `rstar::RTree<TomTomEdgeEntry>`.
pub fn build_tomtom_rtree_in_bbox(
    tomtom_cfg: &TomTomNetworkConfig,
    study_bbox: Rect<f64>,
    buffer_distance_meters: f64,
) -> Result<RTree<TomTomEdgeEntry>, CongestionError> {
    let tt_dir = Path::new(&tomtom_cfg.network_dir);
    let geom_path = tt_dir.join("edges-geometries-enumerated.txt.gz");
    if !geom_path.exists() {
        return Err(CongestionError::Conflation(format!(
            "TomTom geometries file not found at '{}'",
            geom_path.display()
        )));
    }

    let search_bbox = expand_bbox_by_meters(study_bbox, buffer_distance_meters + 50.0);

    // Optional: load time-varying profile IDs if `require_time_varying_trace` is enabled
    // and `speed-profile.csv.gz` + `edges-speed-profile-id-mapping-enumerated.txt.gz` exist.
    let profile_map_path = tt_dir.join("edges-speed-profile-id-mapping-enumerated.txt.gz");
    let speed_profile_path = tt_dir.join("speed-profile.csv.gz");
    let valid_profile_ids: Option<HashSet<usize>> =
        if tomtom_cfg.require_time_varying_trace && speed_profile_path.exists() {
            let profiles: Box<[SpeedProfileCsvRow]> =
                read_utils::from_csv(&speed_profile_path, true, None, None).map_err(|e| {
                    CongestionError::Conflation(format!(
                        "failed reading '{}': {e}",
                        speed_profile_path.display()
                    ))
                })?;
            let valid = profiles
                .iter()
                .filter(|p| {
                    p.monday_trace_id.is_some()
                        || p.tuesday_trace_id.is_some()
                        || p.wednesday_trace_id.is_some()
                        || p.thursday_trace_id.is_some()
                        || p.friday_trace_id.is_some()
                        || p.saturday_trace_id.is_some()
                        || p.sunday_trace_id.is_some()
                })
                .map(|p| p.speed_profile_id)
                .collect::<HashSet<_>>();
            log::info!(
                "Loaded {} time-varying TomTom speed profiles (out of {})",
                valid.len(),
                profiles.len()
            );
            Some(valid)
        } else {
            None
        };

    let mut profile_id_lines = if valid_profile_ids.is_some() && profile_map_path.exists() {
        Some(open_buffered_reader(&profile_map_path)?.lines())
    } else {
        None
    };

    let speed_paths = [
        Path::new(&tomtom_cfg.period_speed_files.wd_am),
        Path::new(&tomtom_cfg.period_speed_files.wd_af),
        Path::new(&tomtom_cfg.period_speed_files.wd_pm),
        Path::new(&tomtom_cfg.period_speed_files.we_am),
        Path::new(&tomtom_cfg.period_speed_files.we_af),
        Path::new(&tomtom_cfg.period_speed_files.we_pm),
    ];
    for p in &speed_paths {
        if !p.exists() {
            return Err(CongestionError::Conflation(format!(
                "TomTom period speed file not found at '{}'",
                p.display()
            )));
        }
    }

    let mut geom_lines = open_buffered_reader(&geom_path)?.lines();
    let mut speed_readers: [std::io::Lines<Box<dyn BufRead>>; 6] = [
        open_buffered_reader(speed_paths[0])?.lines(),
        open_buffered_reader(speed_paths[1])?.lines(),
        open_buffered_reader(speed_paths[2])?.lines(),
        open_buffered_reader(speed_paths[3])?.lines(),
        open_buffered_reader(speed_paths[4])?.lines(),
        open_buffered_reader(speed_paths[5])?.lines(),
    ];

    let unit = tomtom_cfg.speed_unit;
    let mut entries = Vec::new();
    let mut row_idx = 0usize;

    loop {
        let Some(geom_res) = geom_lines.next() else {
            break;
        };
        let cur_idx = row_idx;
        row_idx += 1;

        let mut raw_speeds: [Option<f64>; 6] = [None; 6];
        let mut any_speed = false;
        for (p_idx, reader) in speed_readers.iter_mut().enumerate() {
            if let Some(line_res) = reader.next() {
                let s = line_res.map_err(|source| CongestionError::Io {
                    path: speed_paths[p_idx].to_path_buf(),
                    source,
                })?;
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    if let Ok(val) = trimmed.parse::<f64>() {
                        if val.is_finite() && val > 0.0 {
                            let mph = SpeedUnit::MPH.from_uom(unit.to_uom(val));
                            raw_speeds[p_idx] = Some(mph);
                            any_speed = true;
                        }
                    }
                }
            }
        }

        let profile_line_opt = match profile_id_lines.as_mut() {
            Some(lines) => lines.next(),
            None => None,
        };

        // Skip immediately if no valid speed in any time period
        if !any_speed {
            continue;
        }

        // If `require_time_varying_trace` is enabled, check either the profile ID table
        // or verify that speeds are not completely static across time periods.
        if tomtom_cfg.require_time_varying_trace {
            if let (Some(ref valid_set), Some(Ok(prof_line))) =
                (&valid_profile_ids, profile_line_opt)
            {
                let trimmed = prof_line.trim();
                let is_valid = trimmed
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|pid| valid_set.contains(&pid));
                if !is_valid {
                    continue;
                }
            } else {
                // Fallback check when speed-profile.csv.gz is not present: require variation across periods
                let present: Vec<f64> = raw_speeds.iter().copied().flatten().collect();
                if present.len() >= 2 {
                    let first = present[0];
                    let has_variation = present.iter().any(|v| (*v - first).abs() > 1e-4);
                    if !has_variation {
                        continue;
                    }
                }
            }
        }

        let geom_line = geom_res.map_err(|source| CongestionError::Io {
            path: geom_path.clone(),
            source,
        })?;
        let ls = parse_wkt_linestring(&geom_line, &geom_path, cur_idx)?;
        let Some(rect) = ls.bounding_rect() else {
            continue;
        };

        // Fast bounding-box rejection before computing bearings or inserting into R*-tree
        if rect.max().x < search_bbox.min().x
            || rect.min().x > search_bbox.max().x
            || rect.max().y < search_bbox.min().y
            || rect.min().y > search_bbox.max().y
        {
            continue;
        }

        let Some(mean_heading_deg) = length_weighted_circular_mean_heading_deg(&ls) else {
            continue;
        };

        let bbox = AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]);
        entries.push(TomTomEdgeEntry {
            tomtom_edge_id: cur_idx,
            mean_heading_deg,
            speeds_mph: raw_speeds,
            geometry: ls,
            bbox,
        });
    }

    log::info!(
        "Scanned {} TomTom edges; indexed {} active edges inside study bbox",
        row_idx,
        entries.len()
    );

    Ok(RTree::bulk_load(entries))
}

/// Conflates all high-functional Open Network edges against the TomTom `RTree` in parallel (`rayon`).
pub fn conflate_open_network_with_tomtom(
    open_net: &LoadedOpenNetwork,
    tomtom_rtree: &RTree<TomTomEdgeEntry>,
    conflation_cfg: &ConflationConfig,
) -> Vec<Option<ConflatedEdgeSpeeds>> {
    let buffer_m = conflation_cfg.buffer_distance_meters;
    let max_heading_diff = conflation_cfg.max_heading_diff_degrees;
    let min_overlap_m = conflation_cfg.min_overlap_meters;

    open_net
        .edges
        .par_iter()
        .map(|edge| {
            if !edge.road_class.is_high_functional() {
                return None;
            }
            conflate_single_edge(
                edge,
                tomtom_rtree,
                &open_net.max_speed_by_highway,
                buffer_m,
                max_heading_diff,
                min_overlap_m,
                conflation_cfg,
            )
        })
        .collect()
}

fn conflate_single_edge(
    edge: &OpenNetworkEdge,
    tomtom_rtree: &RTree<TomTomEdgeEntry>,
    max_speed_by_highway: &std::collections::HashMap<String, f64>,
    buffer_m: f64,
    max_heading_diff: f64,
    min_overlap_m: f64,
    conflation_cfg: &ConflationConfig,
) -> Option<ConflatedEdgeSpeeds> {
    let rect = edge.geometry.bounding_rect()?;
    let open_heading = length_weighted_circular_mean_heading_deg(&edge.geometry)?;

    let expanded = expand_bbox_by_meters(rect, buffer_m);
    let query_env = AABB::from_corners(
        [expanded.min().x, expanded.min().y],
        [expanded.max().x, expanded.max().y],
    );

    let center = rect.center();
    let projector = LocalMeterProjector::new(center.x, center.y);
    let mut target_m_opt: Option<LineString<f64>> = None;

    let mut weighted_sums = [0.0_f64; 6];
    let mut weight_totals = [0.0_f64; 6];
    let mut total_overlap_meters = 0.0;
    let mut matched_tomtom_count = 0usize;

    for candidate in tomtom_rtree.locate_in_envelope_intersecting(&query_env) {
        // 1. Cheap wrap-around circular heading filter (<= 45 degrees)
        if circular_heading_diff_deg(open_heading, candidate.mean_heading_deg) > max_heading_diff {
            continue;
        }

        // 2. Project to local tangent-plane meters and compute 5m corridor overlap length
        let target_m =
            target_m_opt.get_or_insert_with(|| projector.project_linestring(&edge.geometry));
        let candidate_m = projector.project_linestring(&candidate.geometry);
        let overlap_m = compute_corridor_overlap_length_meters(target_m, &candidate_m, buffer_m);
        if overlap_m < min_overlap_m {
            continue;
        }

        matched_tomtom_count += 1;
        total_overlap_meters += overlap_m;

        for p_idx in 0..6 {
            if let Some(spd) = candidate.speeds_mph[p_idx] {
                weighted_sums[p_idx] += overlap_m * spd;
                weight_totals[p_idx] += overlap_m;
            }
        }
    }

    if matched_tomtom_count == 0 || total_overlap_meters < min_overlap_m {
        return None;
    }

    // Outlier speed cap matching `prepare_data_for_model` in `calibration_utils.py`:
    // - If posted speed limit exists on the link: cap at `posted_sl + 10 mph`
    // - Else if max speed limit exists for this highway class: cap at `max_class_sl + 5 mph`
    let speed_cap_opt: Option<f64> = if conflation_cfg.cap_speed_outliers {
        if let Some(posted_sl) = edge.posted_speed_mph {
            Some(posted_sl + conflation_cfg.speed_cap_above_posted_mph)
        } else {
            max_speed_by_highway
                .get(&edge.highway_group)
                .map(|max_sl| *max_sl + conflation_cfg.speed_cap_above_class_max_mph)
        }
    } else {
        None
    };

    let mut speeds_mph = [None; 6];
    for p_idx in 0..6 {
        if weight_totals[p_idx] > 0.0 {
            let mut avg = weighted_sums[p_idx] / weight_totals[p_idx];
            if let Some(cap) = speed_cap_opt {
                if avg > cap {
                    avg = cap;
                }
            }
            speeds_mph[p_idx] = Some(avg);
        }
    }

    Some(ConflatedEdgeSpeeds {
        edge_id: edge.edge_id,
        total_overlap_meters,
        matched_tomtom_count,
        speeds_mph,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circular_heading_wrap_around() {
        // 2 degrees vs 358 degrees is a 4-degree difference (not 356!)
        assert!((circular_heading_diff_deg(2.0, 358.0) - 4.0).abs() < 1e-9);
        assert!((circular_heading_diff_deg(358.0, 2.0) - 4.0).abs() < 1e-9);
        // Opposing directions (0 vs 180)
        assert!((circular_heading_diff_deg(0.0, 180.0) - 180.0).abs() < 1e-9);
        assert!((circular_heading_diff_deg(90.0, 270.0) - 180.0).abs() < 1e-9);
    }

    #[test]
    fn test_geodesic_bearing_and_length_weighted_heading() {
        // Due East along equator: (lon=0, lat=0) -> (lon=0.01, lat=0) => 90 degrees
        let east_line = LineString::from(vec![(0.0, 0.0), (0.01, 0.0)]);
        let h_east = length_weighted_circular_mean_heading_deg(&east_line).unwrap();
        assert!(
            (h_east - 90.0).abs() < 1e-3,
            "expected ~90 deg, got {h_east}"
        );

        // Due North: (lon=-105.0, lat=40.0) -> (lon=-105.0, lat=40.01) => 0 degrees
        let north_line = LineString::from(vec![(-105.0, 40.0), (-105.0, 40.01)]);
        let h_north = length_weighted_circular_mean_heading_deg(&north_line).unwrap();
        assert!(
            h_north < 1e-3 || (360.0 - h_north) < 1e-3,
            "expected ~0 deg, got {h_north}"
        );
    }

    #[test]
    fn test_corridor_overlap_parallel_and_perpendicular() {
        let proj = LocalMeterProjector::new(-105.0, 40.0);
        // 100-meter segment going East
        let target = LineString::from(vec![Coord { x: 0.0, y: 0.0 }, Coord { x: 100.0, y: 0.0 }]);
        // Parallel segment 3m North (inside 5m buffer) of length 80m
        let inside = LineString::from(vec![Coord { x: 10.0, y: 3.0 }, Coord { x: 90.0, y: 3.0 }]);
        let overlap_in = compute_corridor_overlap_length_meters(&target, &inside, 5.0);
        assert!((overlap_in - 80.0).abs() < 1e-3);

        // Parallel segment 8m North (outside 5m buffer)
        let outside = LineString::from(vec![Coord { x: 10.0, y: 8.0 }, Coord { x: 90.0, y: 8.0 }]);
        let overlap_out = compute_corridor_overlap_length_meters(&target, &outside, 5.0);
        assert_eq!(overlap_out, 0.0);
        let _ = proj;
    }

    #[test]
    fn test_divided_highway_opposing_carriageway_and_weighted_averaging() {
        use crate::taxonomy::CongestionRoadClass;
        use std::collections::HashMap;

        // Open network edge: Eastbound (~90 deg) from lon -105.002 to -105.000 at lat 40.000
        let open_geom = LineString::from(vec![(-105.0020, 40.0000), (-105.0000, 40.0000)]);
        let open_edge = OpenNetworkEdge {
            edge_id: 0,
            source_id: "osm_100".to_string(),
            src_vertex_id: 0,
            dst_vertex_id: 1,
            length_meters: 170.0,
            highway_group: "motorway".to_string(),
            road_class: CongestionRoadClass::Motorway,
            posted_speed_mph: Some(65.0),
            imputed_speed_limit_mph: 65.0,
            geometry: open_geom,
        };

        // TomTom edge 0: first half Eastbound (~90 deg), 2m north, speed = 50 mph
        let tt_east_1 = LineString::from(vec![(-105.0020, 40.00002), (-105.0010, 40.00002)]);
        // TomTom edge 1: second half Eastbound (~90 deg), 2m north, speed = 70 mph
        let tt_east_2 = LineString::from(vec![(-105.0010, 40.00002), (-105.0000, 40.00002)]);
        // TomTom edge 2: opposing Westbound carriageway (~270 deg), 3m south, speed = 15 mph (congested opposing direction)
        let tt_west_opposing = LineString::from(vec![(-105.0000, 39.99997), (-105.0020, 39.99997)]);

        let make_entry = |id: usize, ls: LineString<f64>, spd: f64| -> TomTomEdgeEntry {
            let rect = ls.bounding_rect().unwrap();
            TomTomEdgeEntry {
                tomtom_edge_id: id,
                mean_heading_deg: length_weighted_circular_mean_heading_deg(&ls).unwrap(),
                speeds_mph: [Some(spd); 6],
                bbox: AABB::from_corners(
                    [rect.min().x, rect.min().y],
                    [rect.max().x, rect.max().y],
                ),
                geometry: ls,
            }
        };

        let rtree = RTree::bulk_load(vec![
            make_entry(0, tt_east_1, 50.0),
            make_entry(1, tt_east_2, 70.0),
            make_entry(2, tt_west_opposing, 15.0),
        ]);

        let max_speeds = HashMap::from([("motorway".to_string(), 75.0)]);
        let cfg = ConflationConfig::default();

        let matched = conflate_single_edge(
            &open_edge,
            &rtree,
            &max_speeds,
            cfg.buffer_distance_meters,
            cfg.max_heading_diff_degrees,
            cfg.min_overlap_meters,
            &cfg,
        )
        .expect("expected Eastbound TomTom edges to match");

        // Only the 2 Eastbound segments should match; the Westbound segment (~270 deg) must be rejected
        assert_eq!(matched.matched_tomtom_count, 2);
        // Equal lengths at 50 mph and 70 mph -> weighted average = 60.0 mph
        let avg_spd = matched.speeds_mph[0].unwrap();
        assert!(
            (avg_spd - 60.0).abs() < 1e-3,
            "expected 60.0 mph, got {avg_spd}"
        );
    }

    #[test]
    fn test_speed_outlier_capping_posted_and_class_max() {
        use crate::taxonomy::CongestionRoadClass;
        use std::collections::HashMap;

        let geom = LineString::from(vec![(-105.0010, 40.0000), (-105.0000, 40.0000)]);
        let rect = geom.bounding_rect().unwrap();
        let tt_entry = TomTomEdgeEntry {
            tomtom_edge_id: 0,
            mean_heading_deg: length_weighted_circular_mean_heading_deg(&geom).unwrap(),
            speeds_mph: [Some(95.0); 6], // Extreme outlier speed (95 mph)
            bbox: AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]),
            geometry: geom.clone(),
        };
        let rtree = RTree::bulk_load(vec![tt_entry]);
        let max_speeds = HashMap::from([("primary".to_string(), 55.0)]);
        let cfg = ConflationConfig::default();

        // Case 1: Edge HAS posted speed limit (45 mph) -> capped at 45 + 10 = 55 mph
        let edge_with_sl = OpenNetworkEdge {
            edge_id: 0,
            source_id: "e0".to_string(),
            src_vertex_id: 0,
            dst_vertex_id: 1,
            length_meters: 85.0,
            highway_group: "primary".to_string(),
            road_class: CongestionRoadClass::Primary,
            posted_speed_mph: Some(45.0),
            imputed_speed_limit_mph: 45.0,
            geometry: geom.clone(),
        };
        let res_with = conflate_single_edge(
            &edge_with_sl,
            &rtree,
            &max_speeds,
            cfg.buffer_distance_meters,
            cfg.max_heading_diff_degrees,
            cfg.min_overlap_meters,
            &cfg,
        )
        .unwrap();
        assert!((res_with.speeds_mph[0].unwrap() - 55.0).abs() < 1e-6);

        // Case 2: Edge is MISSING posted speed limit -> capped at max_speed_by_highway["primary"] (55) + 5 = 60 mph
        let edge_no_sl = OpenNetworkEdge {
            posted_speed_mph: None,
            ..edge_with_sl
        };
        let res_no = conflate_single_edge(
            &edge_no_sl,
            &rtree,
            &max_speeds,
            cfg.buffer_distance_meters,
            cfg.max_heading_diff_degrees,
            cfg.min_overlap_meters,
            &cfg,
        )
        .unwrap();
        assert!((res_no.speeds_mph[0].unwrap() - 60.0).abs() < 1e-6);
    }
}
