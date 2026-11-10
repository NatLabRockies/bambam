use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use bambam_network::{
    consts::{
        CLASSES_FILENAME, COMPASS_EDGES_FILENAME, EDGE_MAPPING_FILENAME, GEOMETRIES_FILENAME,
        POSTED_SPEEDS_FILENAME, SPEEDS_FILENAME,
    },
    EdgeMappingRecord,
};
use bambam_osm::model::osm::import_ops::OsmCollection;
use flate2::read::GzDecoder;
use geo::{BoundingRect, Coord, Geometry, Haversine, Length, LineString, Rect};
use geozero::{wkt::Wkt as WktReader, ToGeo};
use routee_compass_core::{
    model::{network::EdgeConfig, unit::SpeedUnit},
    util::fs::read_utils,
};
use uom::si::f64::Velocity;

use crate::{
    config::OpenNetworkConfig,
    error::CongestionError,
    taxonomy::{normalize_highway_tag, CongestionRoadClass},
};

/// A single road edge loaded from an Open Network (OSM or Overture Maps) in RouteE Compass format.
#[derive(Debug, Clone)]
pub struct OpenNetworkEdge {
    pub edge_id: usize,
    pub source_id: String,
    pub src_vertex_id: usize,
    pub dst_vertex_id: usize,
    pub length_meters: f64,
    /// Normalized highway group string (e.g. `"motorway"`, `"motorway_link"`, `"primary"`, `"residential"`).
    pub highway_group: String,
    /// Consolidated 9-category road class (`Motorway`, `Trunk`, `Link`, `Primary`, `Secondary`, `Tertiary`, `Residential`, `Service`, `Other`).
    pub road_class: CongestionRoadClass,
    /// Raw posted speed limit in mph (`None` if missing on the source way/segment).
    pub posted_speed_mph: Option<f64>,
    /// Imputed speed limit (`sl-med-imp`): posted speed if present, else median speed of `highway_group`, floored at `15.0 mph`.
    pub imputed_speed_limit_mph: f64,
    /// Directed polyline geometry in WGS84 `(lon, lat)` degrees.
    pub geometry: LineString<f64>,
}

impl OpenNetworkEdge {
    /// Returns the speed limit feature passed to the XGBoost model:
    /// `None` if `posted_speed_mph.is_none()` OR if `road_class == CongestionRoadClass::Link`
    /// (matching `models.py` and `predict_speeds_using_models.py`).
    pub fn model_speed_limit_mph(&self) -> Option<f32> {
        if self.road_class.is_link() {
            None
        } else {
            self.posted_speed_mph.map(|s| s as f32)
        }
    }
}

/// Complete loaded Open Network dataset ready for calibration or imputation.
#[derive(Debug, Clone)]
pub struct LoadedOpenNetwork {
    pub root_dir: PathBuf,
    pub mode_dir: PathBuf,
    pub edges: Vec<OpenNetworkEdge>,
    /// Per-`highway_group` median posted speed limit in mph.
    pub median_speed_by_highway: HashMap<String, f64>,
    /// Per-`highway_group` maximum posted speed limit in mph.
    pub max_speed_by_highway: HashMap<String, f64>,
    /// Bounding box `(xmin, ymin) -> (xmax, ymax)` in WGS84 degrees across all edge geometries.
    pub bbox: Rect<f64>,
}

impl LoadedOpenNetwork {
    /// Loads an Open Network from `config`.
    pub fn load(config: &OpenNetworkConfig) -> Result<Self, CongestionError> {
        let root_dir = PathBuf::from(&config.network_dir);
        let candidate_mode_dir = root_dir.join(&config.mode);
        let mode_dir = if candidate_mode_dir.join(COMPASS_EDGES_FILENAME).exists() {
            candidate_mode_dir
        } else if root_dir.join(COMPASS_EDGES_FILENAME).exists() {
            root_dir.clone()
        } else {
            return Err(CongestionError::Network(format!(
                "could not find '{}' in '{}' or '{}'",
                COMPASS_EDGES_FILENAME,
                candidate_mode_dir.display(),
                root_dir.display()
            )));
        };

        log::info!("Loading Open Network from '{}'", mode_dir.display());

        // 1. Load edges-compass.csv.gz
        let edges_compass_path = mode_dir.join(COMPASS_EDGES_FILENAME);
        let compass_edges: Box<[EdgeConfig]> =
            read_utils::from_csv(&edges_compass_path, true, None, None).map_err(|e| {
                CongestionError::Network(format!(
                    "failed reading '{}': {e}",
                    edges_compass_path.display()
                ))
            })?;
        let n_edges = compass_edges.len();

        // 2. Load geometries
        let geom_path = mode_dir.join(GEOMETRIES_FILENAME);
        let geometries = read_wkt_linestrings(&geom_path, Some(n_edges))?;
        if geometries.len() != n_edges {
            return Err(CongestionError::Network(format!(
                "edge count mismatch: '{}' has {} rows, but '{}' has {} rows",
                COMPASS_EDGES_FILENAME,
                n_edges,
                GEOMETRIES_FILENAME,
                geometries.len()
            )));
        }

        // 3. Load road classes
        let classes_path = if mode_dir.join(CLASSES_FILENAME).exists() {
            mode_dir.join(CLASSES_FILENAME)
        } else if mode_dir.join("edges-road-class-enumerated.txt.gz").exists() {
            mode_dir.join("edges-road-class-enumerated.txt.gz")
        } else {
            return Err(CongestionError::Network(format!(
                "missing '{}' (or 'edges-road-class-enumerated.txt.gz') in '{}'",
                CLASSES_FILENAME,
                mode_dir.display()
            )));
        };
        let raw_classes = read_enumerated_lines(&classes_path)?;
        if raw_classes.len() != n_edges {
            return Err(CongestionError::Network(format!(
                "edge count mismatch: '{}' has {} rows, but '{}' has {} rows",
                COMPASS_EDGES_FILENAME,
                n_edges,
                classes_path.display(),
                raw_classes.len()
            )));
        }

        // 4. Load source IDs from edges-mapping.csv.gz or edges-uuid-enumerated.txt.gz
        let edge_mappings_opt = load_edge_mappings(&mode_dir)?;
        let source_ids: Vec<String> = if let Some(ref mappings) = edge_mappings_opt {
            mappings.iter().map(|m| m.source_id.clone()).collect()
        } else if mode_dir.join("edges-uuid-enumerated.txt.gz").exists() {
            read_enumerated_lines(&mode_dir.join("edges-uuid-enumerated.txt.gz"))?
        } else {
            (0..n_edges).map(|i| i.to_string()).collect()
        };

        // 5. Load raw posted speed limits (Option<f64> in mph)
        let posted_speeds_mph = load_posted_speeds_mph(
            &root_dir,
            &mode_dir,
            edge_mappings_opt.as_deref(),
            n_edges,
            config.legacy_posted_speed_unit,
        )?;

        // 6. Normalize highway groups and compute per-highway median & max speed limits
        let highway_groups: Vec<String> = raw_classes
            .iter()
            .map(|c| normalize_highway_tag(c))
            .collect();

        let (median_speed_by_highway, max_speed_by_highway) =
            compute_highway_speed_stats(&highway_groups, &posted_speeds_mph);

        let min_floor = config.min_imputed_speed_limit_mph;

        // 7. Assemble OpenNetworkEdge records & bounding box
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        let mut edges = Vec::with_capacity(n_edges);
        for idx in 0..n_edges {
            let ce = &compass_edges[idx];
            let geom = geometries[idx].clone();
            if let Some(rect) = geom.bounding_rect() {
                min_x = min_x.min(rect.min().x);
                min_y = min_y.min(rect.min().y);
                max_x = max_x.max(rect.max().x);
                max_y = max_y.max(rect.max().y);
            }
            let hwy_group = highway_groups[idx].clone();
            let road_class = CongestionRoadClass::from_class_str(&hwy_group);
            let posted_mph = posted_speeds_mph[idx];
            let raw_imputed = posted_mph
                .or_else(|| median_speed_by_highway.get(&hwy_group).copied())
                .unwrap_or(min_floor);
            let imputed_speed_limit_mph = if raw_imputed.is_nan() || raw_imputed < min_floor {
                min_floor
            } else {
                raw_imputed
            };

            let length_meters = if ce.distance > 0.0 {
                ce.distance
            } else {
                Haversine.length(&geom)
            };

            edges.push(OpenNetworkEdge {
                edge_id: ce.edge_id.0,
                source_id: source_ids
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| idx.to_string()),
                src_vertex_id: ce.src_vertex_id.0,
                dst_vertex_id: ce.dst_vertex_id.0,
                length_meters,
                highway_group: hwy_group,
                road_class,
                posted_speed_mph: posted_mph,
                imputed_speed_limit_mph,
                geometry: geom,
            });
        }

        let bbox = if min_x.is_finite() && min_y.is_finite() {
            Rect::new(Coord { x: min_x, y: min_y }, Coord { x: max_x, y: max_y })
        } else {
            Rect::new(Coord { x: 0.0, y: 0.0 }, Coord { x: 0.0, y: 0.0 })
        };

        let missing_count = edges
            .iter()
            .filter(|e| e.posted_speed_mph.is_none())
            .count();
        let high_func_count = edges
            .iter()
            .filter(|e| e.road_class.is_high_functional())
            .count();
        log::info!(
            "Loaded {} edges ({} high-functional, {} missing posted speed limit)",
            edges.len(),
            high_func_count,
            missing_count
        );

        Ok(Self {
            root_dir,
            mode_dir,
            edges,
            median_speed_by_highway,
            max_speed_by_highway,
            bbox,
        })
    }
}

fn load_edge_mappings(mode_dir: &Path) -> Result<Option<Vec<EdgeMappingRecord>>, CongestionError> {
    let mapping_path = mode_dir.join(EDGE_MAPPING_FILENAME);
    if !mapping_path.exists() {
        return Ok(None);
    }
    // Check if the header matches `EdgeMappingRecord` (`edge_id,source_id,start_lr,end_lr,heading`)
    let res: Result<Box<[EdgeMappingRecord]>, _> =
        read_utils::from_csv(&mapping_path, true, None, None);
    match res {
        Ok(records) => Ok(Some(records.into_vec())),
        Err(_) => Ok(None),
    }
}

fn load_posted_speeds_mph(
    root_dir: &Path,
    mode_dir: &Path,
    edge_mappings: Option<&[EdgeMappingRecord]>,
    n_edges: usize,
    legacy_unit: SpeedUnit,
) -> Result<Vec<Option<f64>>, CongestionError> {
    // 1. Prefer `edges-posted-speeds-mph-enumerated.txt.gz` written by `bambam-network`
    let posted_mph_path = mode_dir.join(POSTED_SPEEDS_FILENAME);
    if posted_mph_path.exists() {
        return read_optional_speeds(&posted_mph_path, SpeedUnit::MPH);
    }

    // 2. If `raw/ways.parquet` exists (`bambam-osm` output) and we have `edge_mappings`,
    //    recover exact raw OSM `maxspeed` before class-average fallback was applied.
    if let Some(mappings) = edge_mappings {
        if root_dir.join("raw").join("ways.parquet").exists() {
            if let Ok(raw_osm) = OsmCollection::from_parquet_dir(root_dir) {
                let mut raw_speed_by_way: HashMap<String, Option<f64>> =
                    HashMap::with_capacity(raw_osm.ways.len());
                for way in &raw_osm.ways {
                    let mph = way
                        .get_speed_value("maxspeed", true)
                        .ok()
                        .flatten()
                        .map(|vel: Velocity| vel.get::<uom::si::velocity::mile_per_hour>());
                    raw_speed_by_way.insert(way.osmid.to_string(), mph);
                }
                let speeds = mappings
                    .iter()
                    .map(|m| raw_speed_by_way.get(&m.source_id).copied().flatten())
                    .collect::<Vec<_>>();
                if speeds.len() == n_edges {
                    return Ok(speeds);
                }
            }
        }
    }

    // 3. Check legacy `edges-complete.csv.gz` + `edges-posted-speed-enumerated.txt.gz`
    //    In legacy OSMnx datasets (`boulder_co/`, `denver_co/`), `edges-posted-speed-enumerated.txt.gz`
    //    had missing speed limits pre-imputed with class averages, whereas `edges-complete.csv.gz`
    //    preserves the raw `maxspeed` column (empty when missing in OSM).
    let legacy_posted_path = mode_dir.join("edges-posted-speed-enumerated.txt.gz");
    if legacy_posted_path.exists() {
        let mut speeds = read_optional_speeds(&legacy_posted_path, legacy_unit)?;
        let complete_edges_path = mode_dir.join("edges-complete.csv.gz");
        if complete_edges_path.exists() {
            if let Ok(mask) = read_raw_maxspeed_presence_mask(&complete_edges_path, n_edges) {
                for (s, has_raw) in speeds.iter_mut().zip(mask.into_iter()) {
                    if !has_raw {
                        *s = None;
                    }
                }
            }
        }
        return Ok(speeds);
    }

    // 4. Fall back to `edges-speeds-mph-enumerated.txt.gz`
    let speeds_mph_path = mode_dir.join(SPEEDS_FILENAME);
    if speeds_mph_path.exists() {
        return read_optional_speeds(&speeds_mph_path, SpeedUnit::MPH);
    }

    Ok(vec![None; n_edges])
}

/// Checks `edges-complete.csv.gz` for a `maxspeed` column and returns a boolean mask
/// indicating which rows have a non-empty raw OSM `maxspeed` tag.
fn read_raw_maxspeed_presence_mask(
    complete_edges_path: &Path,
    expected_len: usize,
) -> Result<Vec<bool>, CongestionError> {
    let reader = open_buffered_reader(complete_edges_path)?;
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);
    let headers = csv_reader
        .headers()
        .map_err(|e| CongestionError::Csv {
            path: complete_edges_path.to_path_buf(),
            message: e.to_string(),
        })?
        .clone();
    let Some(maxspeed_idx) = headers
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case("maxspeed"))
    else {
        return Err(CongestionError::Network(
            "no maxspeed column in edges-complete.csv.gz".to_string(),
        ));
    };

    let mut mask = Vec::with_capacity(expected_len);
    for rec_res in csv_reader.records() {
        let rec = rec_res.map_err(|e| CongestionError::Csv {
            path: complete_edges_path.to_path_buf(),
            message: e.to_string(),
        })?;
        let has_val = rec.get(maxspeed_idx).map(|s| s.trim()).is_some_and(|s| {
            !s.is_empty()
                && !s.eq_ignore_ascii_case("nan")
                && !s.eq_ignore_ascii_case("none")
                && s != "-299"
        });
        mask.push(has_val);
    }
    if mask.len() != expected_len {
        return Err(CongestionError::Network(format!(
            "edges-complete.csv.gz row count {} != expected {}",
            mask.len(),
            expected_len
        )));
    }
    Ok(mask)
}

/// Reads an enumerated speed file (`.txt` or `.txt.gz`) and converts valid positive speeds
/// from `source_unit` into miles per hour (`mph`). Empty lines, `NaN`, `<= 0.0`, and `-299`
/// (the legacy OSM missing speed sentinel) are mapped to `None`.
pub fn read_optional_speeds(
    path: &Path,
    source_unit: SpeedUnit,
) -> Result<Vec<Option<f64>>, CongestionError> {
    let lines = read_enumerated_lines(path)?;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("nan")
            || trimmed.eq_ignore_ascii_case("none")
        {
            out.push(None);
            continue;
        }
        match trimmed.parse::<f64>() {
            Ok(val) if val.is_finite() && val > 0.0 && (val - -299.0).abs() > 1e-6 => {
                let vel = source_unit.to_uom(val);
                let mph = SpeedUnit::MPH.from_uom(vel);
                out.push(Some(mph));
            }
            _ => out.push(None),
        }
    }
    Ok(out)
}

/// Reads all lines from a plain text or `.gz` file.
pub fn read_enumerated_lines(path: &Path) -> Result<Vec<String>, CongestionError> {
    let reader = open_buffered_reader(path)?;
    let mut lines = Vec::new();
    for line_res in reader.lines() {
        let line = line_res.map_err(|source| CongestionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        lines.push(line);
    }
    Ok(lines)
}

/// Opens a `BufRead` stream over either a plain file or a `.gz`-compressed file.
pub fn open_buffered_reader(path: &Path) -> Result<Box<dyn BufRead>, CongestionError> {
    let file = File::open(path).map_err(|source| CongestionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let is_gz = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));
    if is_gz {
        Ok(Box::new(BufReader::new(GzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Reads WKT `LINESTRING` geometries from an enumerated `.txt` or `.txt.gz` file.
pub fn read_wkt_linestrings(
    path: &Path,
    capacity_hint: Option<usize>,
) -> Result<Vec<LineString<f64>>, CongestionError> {
    let reader = open_buffered_reader(path)?;
    let mut out = Vec::with_capacity(capacity_hint.unwrap_or(0));
    for (idx, line_res) in reader.lines().enumerate() {
        let line = line_res.map_err(|source| CongestionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let ls = parse_wkt_linestring(&line, path, idx)?;
        out.push(ls);
    }
    Ok(out)
}

/// Parses a single WKT `LINESTRING` string into `geo::LineString<f64>`.
pub fn parse_wkt_linestring(
    raw: &str,
    path: &Path,
    row_idx: usize,
) -> Result<LineString<f64>, CongestionError> {
    let trimmed = raw.trim().trim_matches('"');
    let geom = WktReader(trimmed).to_geo().map_err(|e| {
        CongestionError::Geometry(format!(
            "failed parsing WKT at '{}' row {}: {e}",
            path.display(),
            row_idx
        ))
    })?;
    match geom {
        Geometry::LineString(ls) => Ok(ls),
        Geometry::MultiLineString(mut mls) if !mls.0.is_empty() => Ok(mls.0.remove(0)),
        other => Err(CongestionError::Geometry(format!(
            "expected LineString at '{}' row {}, got {other:?}",
            path.display(),
            row_idx
        ))),
    }
}

/// Computes per-highway-group median posted speed (`sl-med-imp`) and maximum posted speed.
pub fn compute_highway_speed_stats(
    highway_groups: &[String],
    posted_speeds_mph: &[Option<f64>],
) -> (HashMap<String, f64>, HashMap<String, f64>) {
    let mut grouped: HashMap<String, Vec<f64>> = HashMap::new();
    for (hwy, opt_speed) in highway_groups.iter().zip(posted_speeds_mph.iter()) {
        if let Some(speed) = opt_speed {
            if speed.is_finite() && *speed > 0.0 {
                grouped.entry(hwy.clone()).or_default().push(*speed);
            }
        }
    }

    let mut medians = HashMap::with_capacity(grouped.len());
    let mut maxes = HashMap::with_capacity(grouped.len());
    for (hwy, mut vals) in grouped {
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = vals.len();
        if n > 0 {
            let median = if n % 2 == 1 {
                vals[n / 2]
            } else {
                0.5 * (vals[n / 2 - 1] + vals[n / 2])
            };
            let max_val = vals[n - 1];
            medians.insert(hwy.clone(), median);
            maxes.insert(hwy, max_val);
        }
    }
    (medians, maxes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_highway_speed_stats_and_link_masking() {
        let highways = vec![
            "primary".to_string(),
            "primary".to_string(),
            "primary".to_string(),
            "motorway_link".to_string(),
            "motorway_link".to_string(),
        ];
        let speeds = vec![Some(30.0), Some(45.0), Some(35.0), Some(50.0), None];
        let (medians, maxes) = compute_highway_speed_stats(&highways, &speeds);

        assert!((medians["primary"] - 35.0).abs() < 1e-9);
        assert!((maxes["primary"] - 45.0).abs() < 1e-9);
        assert!((medians["motorway_link"] - 50.0).abs() < 1e-9);

        // Verify that a Link road masks model_speed_limit_mph() to None even when posted_speed_mph is Some
        let link_edge = OpenNetworkEdge {
            edge_id: 0,
            source_id: "w1".to_string(),
            src_vertex_id: 0,
            dst_vertex_id: 1,
            length_meters: 100.0,
            highway_group: "motorway_link".to_string(),
            road_class: CongestionRoadClass::Link,
            posted_speed_mph: Some(50.0),
            imputed_speed_limit_mph: 50.0,
            geometry: LineString::from(vec![(-105.0, 40.0), (-104.999, 40.0)]),
        };
        assert_eq!(link_edge.model_speed_limit_mph(), None);
        assert!((link_edge.imputed_speed_limit_mph - 50.0).abs() < 1e-9);
    }
}
