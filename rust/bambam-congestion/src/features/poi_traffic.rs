use std::{collections::HashMap, path::Path};

use bambam_network::{consts::COMPLETE_VERTEX_FILENAME, VertexCompassRecord, VertexCompleteRecord};
use geo::{BoundingRect, Coord, Euclidean, Length, LineString, Rect};
use geozero::{wkt::Wkt as WktReader, ToGeo};
use osmpbf::{Element, ElementReader};
use routee_compass_core::util::fs::read_utils;
use rstar::{RTree, RTreeObject, AABB};

use crate::{
    config::{
        OpportunitiesSourceConfig, ProjectionMode, SpatialFeaturesConfig,
        TrafficControlsSourceConfig,
    },
    conflate::{expand_bbox_by_meters, point_to_linestring_distance_m, LocalMeterProjector},
    error::CongestionError,
    network::{open_buffered_reader, LoadedOpenNetwork, OpenNetworkEdge},
};

const METERS_PER_MILE: f64 = 1609.344;
const LEGACY_METERS_PER_MILE: f64 = 1609.34;
const SQ_MILES_PER_SQ_METER_LEGACY: f64 = 3.86102e-7;
const WEB_MERCATOR_RADIUS: f64 = 6_378_137.0;

/// Kind of spatial point feature indexed in the unified 0.5-mile R*-tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureKind {
    /// Land-use opportunity (`sch-day-re`, `medical`, `shop-err`, `recreation`, `meals`).
    Opportunity,
    /// Traffic control (`tra-signal`, `crossing`, `tra-stop`).
    TrafficControl,
}

/// A point feature (in WGS84 `lon, lat`) stored in the unified `rstar::RTree`.
#[derive(Debug, Clone)]
pub struct SpatialFeaturePoint {
    pub lon: f64,
    pub lat: f64,
    pub kind: FeatureKind,
    /// Multiplicity weight (e.g., `1.0` for a single POI, `2.0` for a combined signal+crossing node,
    /// or total opportunity count at a vertex).
    pub weight: f64,
}

impl RTreeObject for SpatialFeaturePoint {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point([self.lon, self.lat])
    }
}

/// Projects a WGS84 `(lon, lat)` coordinate into `EPSG:3857` Web Mercator planar units.
#[inline]
pub fn lonlat_to_epsg3857(c: Coord<f64>) -> Coord<f64> {
    let x = WEB_MERCATOR_RADIUS * c.x.to_radians();
    let lat_clamped = c.y.clamp(-85.05112878, 85.05112878);
    let y = WEB_MERCATOR_RADIUS
        * (std::f64::consts::FRAC_PI_4 + 0.5 * lat_clamped.to_radians())
            .tan()
            .ln();
    Coord { x, y }
}

/// Builds the unified `RTree<SpatialFeaturePoint>` containing both MEP opportunity points
/// and traffic control points within the expanded study bounding box.
pub fn build_poi_and_traffic_rtree(
    open_net: &LoadedOpenNetwork,
    cfg: &SpatialFeaturesConfig,
) -> Result<RTree<SpatialFeaturePoint>, CongestionError> {
    let radius_m = cfg.density_radius_miles * METERS_PER_MILE;
    let search_bbox = expand_bbox_by_meters(open_net.bbox, radius_m * 1.5 + 200.0);

    let mut points: Vec<SpatialFeaturePoint> = Vec::new();
    let mut loaded_traffic_from_pbf = false;

    // 1. Load MEP opportunity points
    if let Some(ref opp_src) = cfg.opportunities_source {
        match opp_src {
            OpportunitiesSourceConfig::OsmPbf { pbf_path } => {
                // If traffic_controls_source is None or also OsmPbf with the same path,
                // extract both opportunities and traffic controls in a single PBF pass!
                let extract_traffic = match &cfg.traffic_controls_source {
                    None => !open_net.root_dir.join(COMPLETE_VERTEX_FILENAME).exists(),
                    Some(TrafficControlsSourceConfig::OsmPbf { pbf_path: tp }) => tp == pbf_path,
                    _ => false,
                };
                let mut pbf_pts = extract_osm_pbf_features(
                    Path::new(pbf_path),
                    search_bbox,
                    true,
                    extract_traffic,
                )?;
                if extract_traffic {
                    loaded_traffic_from_pbf = true;
                }
                points.append(&mut pbf_pts);
            }
            OpportunitiesSourceConfig::CsvPoints {
                path,
                x_column,
                y_column,
                wkt_column,
                weight_column,
            } => {
                let mut csv_pts = load_csv_feature_points(
                    Path::new(path),
                    x_column,
                    y_column,
                    wkt_column.as_deref(),
                    weight_column.as_deref(),
                    FeatureKind::Opportunity,
                    search_bbox,
                )?;
                points.append(&mut csv_pts);
            }
            OpportunitiesSourceConfig::VertexOpportunitiesCsv {
                opportunities_csv_path,
                columns,
            } => {
                let mut v_pts = load_vertex_opportunities_csv(
                    &open_net.root_dir,
                    Path::new(opportunities_csv_path),
                    columns.as_deref(),
                    search_bbox,
                )?;
                points.append(&mut v_pts);
            }
        }
    }

    // 2. Load traffic control points (`traffic_signals`, `stop`, `crossing`)
    if !loaded_traffic_from_pbf {
        match &cfg.traffic_controls_source {
            None => {
                let default_vc = open_net.root_dir.join(COMPLETE_VERTEX_FILENAME);
                let mode_vc = open_net.mode_dir.join(COMPLETE_VERTEX_FILENAME);
                let vc_path = if default_vc.exists() {
                    Some(default_vc)
                } else if mode_vc.exists() {
                    Some(mode_vc)
                } else {
                    None
                };
                if let Some(path) = vc_path {
                    let mut tc_pts = load_traffic_controls_from_vertices_complete(&path, search_bbox)?;
                    points.append(&mut tc_pts);
                }
            }
            Some(TrafficControlsSourceConfig::VerticesComplete { path }) => {
                let resolved = path
                    .as_ref()
                    .map(Path::new)
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| open_net.root_dir.join(COMPLETE_VERTEX_FILENAME));
                let mut tc_pts = load_traffic_controls_from_vertices_complete(&resolved, search_bbox)?;
                points.append(&mut tc_pts);
            }
            Some(TrafficControlsSourceConfig::OsmPbf { pbf_path }) => {
                let mut tc_pts =
                    extract_osm_pbf_features(Path::new(pbf_path), search_bbox, false, true)?;
                points.append(&mut tc_pts);
            }
            Some(TrafficControlsSourceConfig::CsvPoints {
                path,
                x_column,
                y_column,
            }) => {
                let mut tc_pts = load_csv_feature_points(
                    Path::new(path),
                    x_column,
                    y_column,
                    None,
                    None,
                    FeatureKind::TrafficControl,
                    search_bbox,
                )?;
                points.append(&mut tc_pts);
            }
        }
    }

    let opp_cnt = points
        .iter()
        .filter(|p| p.kind == FeatureKind::Opportunity)
        .count();
    let tc_cnt = points
        .iter()
        .filter(|p| p.kind == FeatureKind::TrafficControl)
        .count();
    log::info!(
        "Indexed {} opportunity points and {} traffic control points into R*-tree",
        opp_cnt,
        tc_cnt
    );

    Ok(RTree::bulk_load(points))
}

/// Computes `(opp_density_per_sq_mi, traffic_control_density_per_sq_mi)` for a single edge
/// in a single R*-tree pass with zero polygon allocations.
pub fn compute_edge_poi_and_traffic_densities(
    edge: &OpenNetworkEdge,
    rtree: &RTree<SpatialFeaturePoint>,
    density_radius_miles: f64,
    projection_mode: ProjectionMode,
) -> (f32, f32) {
    let Some(rect) = edge.geometry.bounding_rect() else {
        return (0.0, 0.0);
    };

    match projection_mode {
        ProjectionMode::Geodesic => {
            let radius_m = density_radius_miles * METERS_PER_MILE;
            let expanded = expand_bbox_by_meters(rect, radius_m);
            let env = AABB::from_corners(
                [expanded.min().x, expanded.min().y],
                [expanded.max().x, expanded.max().y],
            );

            let center = rect.center();
            let projector = LocalMeterProjector::new(center.x, center.y);
            let edge_m = projector.project_linestring(&edge.geometry);

            let length_mi = edge.length_meters / METERS_PER_MILE;
            let area_sq_mi = std::f64::consts::PI * density_radius_miles * density_radius_miles
                + 2.0 * density_radius_miles * length_mi;
            if area_sq_mi <= 1e-12 {
                return (0.0, 0.0);
            }

            let mut opp_sum = 0.0;
            let mut tc_sum = 0.0;

            for pt in rtree.locate_in_envelope(&env) {
                let pt_m = projector.project_coord(Coord {
                    x: pt.lon,
                    y: pt.lat,
                });
                if point_to_linestring_dist(pt_m, &edge_m) <= radius_m {
                    match pt.kind {
                        FeatureKind::Opportunity => opp_sum += pt.weight,
                        FeatureKind::TrafficControl => tc_sum += pt.weight,
                    }
                }
            }

            (
                (opp_sum / area_sq_mi) as f32,
                (tc_sum / area_sq_mi) as f32,
            )
        }
        ProjectionMode::LegacyEpsg3857 => {
            // Legacy EPSG:3857 planar buffer & area matching calibration_utils.py
            let radius_3857 = density_radius_miles * LEGACY_METERS_PER_MILE;
            // In EPSG:3857, a planar buffer of radius_3857 covers at most radius_3857 ground meters
            let expanded = expand_bbox_by_meters(rect, radius_3857);
            let env = AABB::from_corners(
                [expanded.min().x, expanded.min().y],
                [expanded.max().x, expanded.max().y],
            );

            let edge_3857 = LineString(
                edge.geometry
                    .0
                    .iter()
                    .map(|c| lonlat_to_epsg3857(*c))
                    .collect(),
            );
            let length_3857 = Euclidean.length(&edge_3857);
            let area_sq_mi = (std::f64::consts::PI * radius_3857 * radius_3857
                + 2.0 * radius_3857 * length_3857)
                * SQ_MILES_PER_SQ_METER_LEGACY;
            if area_sq_mi <= 1e-12 {
                return (0.0, 0.0);
            }

            let mut opp_sum = 0.0;
            let mut tc_sum = 0.0;

            for pt in rtree.locate_in_envelope(&env) {
                let pt_3857 = lonlat_to_epsg3857(Coord {
                    x: pt.lon,
                    y: pt.lat,
                });
                if point_to_linestring_dist(pt_3857, &edge_3857) <= radius_3857 {
                    match pt.kind {
                        FeatureKind::Opportunity => opp_sum += pt.weight,
                        FeatureKind::TrafficControl => tc_sum += pt.weight,
                    }
                }
            }

            (
                (opp_sum / area_sq_mi) as f32,
                (tc_sum / area_sq_mi) as f32,
            )
        }
    }
}

#[inline]
fn point_to_linestring_dist(pt: Coord<f64>, ls: &LineString<f64>) -> f64 {
    if ls.0.is_empty() {
        return f64::INFINITY;
    }
    if ls.0.len() == 1 {
        let dx = pt.x - ls.0[0].x;
        let dy = pt.y - ls.0[0].y;
        return (dx * dx + dy * dy).sqrt();
    }
    point_to_linestring_distance_m(pt, ls)
}

fn load_traffic_controls_from_vertices_complete(
    path: &Path,
    search_bbox: Rect<f64>,
) -> Result<Vec<SpatialFeaturePoint>, CongestionError> {
    let vertices: Box<[VertexCompleteRecord]> =
        read_utils::from_csv(&path, true, None, None).map_err(|e| {
            CongestionError::SpatialFeature(format!(
                "failed reading '{}': {e}",
                path.display()
            ))
        })?;
    let mut out = Vec::new();
    for v in vertices.iter() {
        let lon = v.x as f64;
        let lat = v.y as f64;
        if lon < search_bbox.min().x
            || lon > search_bbox.max().x
            || lat < search_bbox.min().y
            || lat > search_bbox.max().y
        {
            continue;
        }
        let weight = (v.traffic_signals as u8 + v.stop as u8 + v.crossing as u8) as f64;
        if weight > 0.0 {
            out.push(SpatialFeaturePoint {
                lon,
                lat,
                kind: FeatureKind::TrafficControl,
                weight,
            });
        }
    }
    Ok(out)
}

fn load_vertex_opportunities_csv(
    root_dir: &Path,
    opportunities_csv_path: &Path,
    columns: Option<&[String]>,
    search_bbox: Rect<f64>,
) -> Result<Vec<SpatialFeaturePoint>, CongestionError> {
    let vertices_path = root_dir.join("vertices-compass.csv.gz");
    let vertices: Box<[VertexCompassRecord]> =
        read_utils::from_csv(&vertices_path, true, None, None).map_err(|e| {
            CongestionError::SpatialFeature(format!(
                "failed reading '{}': {e}",
                vertices_path.display()
            ))
        })?;

    let reader = open_buffered_reader(opportunities_csv_path)?;
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let headers = csv_reader
        .headers()
        .map_err(|e| CongestionError::Csv {
            path: opportunities_csv_path.to_path_buf(),
            message: e.to_string(),
        })?
        .clone();

    let col_indices: Vec<usize> = match columns {
        Some(wanted) => headers
            .iter()
            .enumerate()
            .filter_map(|(i, name)| {
                if wanted.iter().any(|w| w.eq_ignore_ascii_case(name)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect(),
        None => (0..headers.len()).collect(),
    };

    let mut out = Vec::new();
    for (row_idx, rec_res) in csv_reader.records().enumerate() {
        let Some(v) = vertices.get(row_idx) else {
            break;
        };
        let lon = v.x as f64;
        let lat = v.y as f64;
        if lon < search_bbox.min().x
            || lon > search_bbox.max().x
            || lat < search_bbox.min().y
            || lat > search_bbox.max().y
        {
            continue;
        }
        let rec = rec_res.map_err(|e| CongestionError::Csv {
            path: opportunities_csv_path.to_path_buf(),
            message: e.to_string(),
        })?;
        let mut total = 0.0;
        for &c_idx in &col_indices {
            if let Some(val_str) = rec.get(c_idx) {
                if let Ok(val) = val_str.trim().parse::<f64>() {
                    if val > 0.0 && val.is_finite() {
                        total += val;
                    }
                }
            }
        }
        if total > 0.0 {
            out.push(SpatialFeaturePoint {
                lon,
                lat,
                kind: FeatureKind::Opportunity,
                weight: total,
            });
        }
    }
    Ok(out)
}

fn load_csv_feature_points(
    path: &Path,
    x_column: &str,
    y_column: &str,
    wkt_column: Option<&str>,
    weight_column: Option<&str>,
    kind: FeatureKind,
    search_bbox: Rect<f64>,
) -> Result<Vec<SpatialFeaturePoint>, CongestionError> {
    let reader = open_buffered_reader(path)?;
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);
    let headers = csv_reader
        .headers()
        .map_err(|e| CongestionError::Csv {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?
        .clone();

    let header_map: HashMap<&str, usize> = headers.iter().enumerate().map(|(i, h)| (h, i)).collect();
    let wkt_idx = wkt_column.and_then(|c| header_map.get(c).copied());
    let x_idx = header_map.get(x_column).copied();
    let y_idx = header_map.get(y_column).copied();
    let weight_idx = weight_column.and_then(|c| header_map.get(c).copied());

    let mut out = Vec::new();
    for rec_res in csv_reader.records() {
        let rec = rec_res.map_err(|e| CongestionError::Csv {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        let coord_opt: Option<(f64, f64)> = if let Some(wi) = wkt_idx {
            rec.get(wi).and_then(|wkt_str| {
                WktReader(wkt_str.trim())
                    .to_geo()
                    .ok()
                    .and_then(|g| g.bounding_rect().map(|r| (r.center().x, r.center().y)))
            })
        } else if let (Some(xi), Some(yi)) = (x_idx, y_idx) {
            let x = rec.get(xi).and_then(|s| s.trim().parse::<f64>().ok());
            let y = rec.get(yi).and_then(|s| s.trim().parse::<f64>().ok());
            x.zip(y)
        } else {
            None
        };

        let Some((lon, lat)) = coord_opt else {
            continue;
        };
        if lon < search_bbox.min().x
            || lon > search_bbox.max().x
            || lat < search_bbox.min().y
            || lat > search_bbox.max().y
        {
            continue;
        }
        let weight = weight_idx
            .and_then(|wi| rec.get(wi))
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(1.0);
        if weight > 0.0 {
            out.push(SpatialFeaturePoint {
                lon,
                lat,
                kind,
                weight,
            });
        }
    }
    Ok(out)
}

/// Extracts MEP land-use opportunities and/or traffic control points directly from a `.osm.pbf`
/// file in a single streaming pass (no Overpass API calls).
pub fn extract_osm_pbf_features(
    pbf_path: &Path,
    search_bbox: Rect<f64>,
    extract_opportunities: bool,
    extract_traffic_controls: bool,
) -> Result<Vec<SpatialFeaturePoint>, CongestionError> {
    let reader = ElementReader::from_path(pbf_path).map_err(|e| {
        CongestionError::SpatialFeature(format!(
            "failed opening PBF '{}': {e}",
            pbf_path.display()
        ))
    })?;

    // Map node_id -> (lon, lat) inside expanded bbox for computing way centroids
    let mut node_coords: HashMap<i64, (f32, f32)> = HashMap::new();
    let mut features: Vec<SpatialFeaturePoint> = Vec::new();
    let mut pending_ways: Vec<(Vec<i64>, f64)> = Vec::new();

    reader
        .for_each(|elem| match elem {
            Element::Node(node) => {
                let lon = node.lon();
                let lat = node.lat();
                if lon >= search_bbox.min().x
                    && lon <= search_bbox.max().x
                    && lat >= search_bbox.min().y
                    && lat <= search_bbox.max().y
                {
                    node_coords.insert(node.id(), (lon as f32, lat as f32));
                    process_osm_tags(
                        lon,
                        lat,
                        node.tags(),
                        extract_opportunities,
                        extract_traffic_controls,
                        &mut features,
                    );
                }
            }
            Element::DenseNode(node) => {
                let lon = node.lon();
                let lat = node.lat();
                if lon >= search_bbox.min().x
                    && lon <= search_bbox.max().x
                    && lat >= search_bbox.min().y
                    && lat <= search_bbox.max().y
                {
                    node_coords.insert(node.id(), (lon as f32, lat as f32));
                    process_osm_tags(
                        lon,
                        lat,
                        node.tags(),
                        extract_opportunities,
                        extract_traffic_controls,
                        &mut features,
                    );
                }
            }
            Element::Way(way) if extract_opportunities => {
                let opp_count = count_mep_opportunity_categories(way.tags());
                if opp_count > 0 {
                    let refs: Vec<i64> = way.refs().collect();
                    if !refs.is_empty() {
                        pending_ways.push((refs, opp_count as f64));
                    }
                }
            }
            _ => {}
        })
        .map_err(|e| {
            CongestionError::SpatialFeature(format!(
                "error reading PBF '{}': {e}",
                pbf_path.display()
            ))
        })?;

    for (refs, weight) in pending_ways {
        let mut sum_x = 0.0_f64;
        let mut sum_y = 0.0_f64;
        let mut cnt = 0usize;
        for nid in refs {
            if let Some(&(lon, lat)) = node_coords.get(&nid) {
                sum_x += lon as f64;
                sum_y += lat as f64;
                cnt += 1;
            }
        }
        if cnt > 0 {
            features.push(SpatialFeaturePoint {
                lon: sum_x / (cnt as f64),
                lat: sum_y / (cnt as f64),
                kind: FeatureKind::Opportunity,
                weight,
            });
        }
    }

    Ok(features)
}

fn process_osm_tags<'a, I>(
    lon: f64,
    lat: f64,
    tags: I,
    extract_opportunities: bool,
    extract_traffic_controls: bool,
    out: &mut Vec<SpatialFeaturePoint>,
) where
    I: IntoIterator<Item = (&'a str, &'a str)> + Clone,
{
    if extract_opportunities {
        let opp_cnt = count_mep_opportunity_categories(tags.clone());
        if opp_cnt > 0 {
            out.push(SpatialFeaturePoint {
                lon,
                lat,
                kind: FeatureKind::Opportunity,
                weight: opp_cnt as f64,
            });
        }
    }

    if extract_traffic_controls {
        let tc_cnt = count_traffic_control_node(tags);
        if tc_cnt > 0 {
            out.push(SpatialFeaturePoint {
                lon,
                lat,
                kind: FeatureKind::TrafficControl,
                weight: tc_cnt as f64,
            });
        }
    }
}

/// Counts how many distinct MEP opportunity categories (`specialty`, `retail`, `food`,
/// `health care`, `recreation`) an OSM element matches, following `get_osm_opportunities`
/// in `calibration_utils.py`.
fn count_mep_opportunity_categories<'a, I>(tags: I) -> u8
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    // Bitmask: bit 0 = specialty, bit 1 = retail, bit 2 = food, bit 3 = health care, bit 4 = recreation
    let mut mask: u8 = 0;
    let mut is_picnic_table = false;

    for (k, v) in tags {
        match k {
            "leisure" => {
                if v == "picnic_table" {
                    is_picnic_table = true;
                } else {
                    mask |= 1 << 4; // recreation
                }
            }
            "shop" => mask |= 1 << 1,       // retail
            "healthcare" => mask |= 1 << 3, // health care
            "education" | "religion" => mask |= 1 << 0, // specialty
            "cuisine" => mask |= 1 << 2,    // food
            "sport" => mask |= 1 << 4,      // recreation
            "amenity" => {
                if let Some(bit) = classify_osm_amenity(v) {
                    mask |= 1 << bit;
                }
            }
            "building" => {
                if let Some(bit) = classify_osm_building(v) {
                    mask |= 1 << bit;
                }
            }
            "tourism" => {
                if let Some(bit) = classify_osm_tourism(v) {
                    mask |= 1 << bit;
                }
            }
            "historic" => {
                if classify_osm_historic(v) {
                    mask |= 1 << 4; // recreation
                }
            }
            _ => {}
        }
    }

    if is_picnic_table {
        0
    } else {
        mask.count_ones() as u8
    }
}

fn classify_osm_amenity(v: &str) -> Option<u8> {
    match v {
        // food (bit 2)
        "restaurant" | "fast_food" | "cafe" | "bar" | "pub" | "ice_cream" | "food_court"
        | "biergarten" => Some(2),
        // health care (bit 3)
        "hospital" | "clinic" | "doctors" | "dentist" | "pharmacy" | "nursing_home"
        | "veterinary" => Some(3),
        // specialty (bit 0: school / day care / religious / civic)
        "school" | "university" | "college" | "kindergarten" | "childcare" | "library"
        | "place_of_worship" | "community_centre" | "social_facility" | "townhall"
        | "courthouse" | "post_office" | "police" | "fire_station" => Some(0),
        // retail / errands (bit 1)
        "bank" | "atm" | "marketplace" | "fuel" | "car_wash" | "car_rental" | "car_repair"
        | "bureau_de_change" | "veterinary_pharmacy" => Some(1),
        // recreation (bit 4)
        "arts_centre" | "cinema" | "theatre" | "nightclub" | "casino" | "planetarium"
        | "events_venue" | "public_bath" | "fountain" => Some(4),
        _ => None,
    }
}

fn classify_osm_building(v: &str) -> Option<u8> {
    match v {
        "retail" | "supermarket" | "kiosk" | "commercial" => Some(1),
        "hospital" => Some(3),
        "school" | "university" | "college" | "kindergarten" | "church" | "cathedral"
        | "chapel" | "mosque" | "synagogue" | "temple" | "shrine" | "civic" | "public" => Some(0),
        "stadium" | "Sports_hall" | "sports_hall" | "grandstand" | "pavilion" => Some(4),
        _ => None,
    }
}

fn classify_osm_tourism(v: &str) -> Option<u8> {
    match v {
        "museum" | "gallery" | "artwork" | "attraction" | "theme_park" | "zoo" | "aquarium"
        | "viewpoint" | "picnic_site" | "camp_site" => Some(4),
        "hotel" | "motel" | "hostel" | "guest_house" | "chalet" | "apartment" => Some(1),
        _ => None,
    }
}

fn classify_osm_historic(v: &str) -> bool {
    matches!(
        v,
        "monument"
            | "memorial"
            | "castle"
            | "ruins"
            | "archaeological_site"
            | "fort"
            | "manor"
            | "historic"
            | "yes"
    )
}

/// Counts traffic control features (`tra-signal`, `crossing`, `tra-stop`) on an OSM node,
/// matching `get_osm_traffic_features` in `calibration_utils.py` (excluding `crossing=informal`
/// and `street_lamp`, and counting `highway=crossing` + `crossing=traffic_signals` as `2`).
fn count_traffic_control_node<'a, I>(tags: I) -> u8
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut hwy: Option<&str> = None;
    let mut crossing: Option<&str> = None;
    for (k, v) in tags {
        match k {
            "highway" => hwy = Some(v),
            "crossing" => crossing = Some(v),
            _ => {}
        }
    }
    if crossing == Some("informal") {
        return 0;
    }
    match hwy {
        Some("traffic_signals") | Some("stop") => 1,
        Some("crossing") => {
            if crossing == Some("traffic_signals") {
                2
            } else {
                1
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osm_traffic_control_counting() {
        // highway=crossing + crossing=traffic_signals counts as 2 (both crossing and signal)
        assert_eq!(
            count_traffic_control_node([("highway", "crossing"), ("crossing", "traffic_signals")]),
            2
        );
        // informal crossing is ignored
        assert_eq!(
            count_traffic_control_node([("highway", "crossing"), ("crossing", "informal")]),
            0
        );
        // street_lamp is ignored (not used by model)
        assert_eq!(
            count_traffic_control_node([("highway", "street_lamp")]),
            0
        );
        // stop sign counts as 1
        assert_eq!(count_traffic_control_node([("highway", "stop")]), 1);
    }

    #[test]
    fn test_osm_mep_opportunity_counting() {
        assert_eq!(
            count_mep_opportunity_categories([("amenity", "restaurant")]),
            1
        );
        // picnic_table is excluded
        assert_eq!(
            count_mep_opportunity_categories([("leisure", "picnic_table")]),
            0
        );
        // Element with both shop and cuisine matches 2 categories (retail + food)
        assert_eq!(
            count_mep_opportunity_categories([("shop", "bakery"), ("cuisine", "french")]),
            2
        );
    }

    #[test]
    fn test_analytical_buffer_area_and_radius_density() {
        use crate::taxonomy::CongestionRoadClass;

        // Straight East-West edge at lat 40.0 of length 1609.344m (1.0 mile)
        let edge = OpenNetworkEdge {
            edge_id: 0,
            source_id: "e0".to_string(),
            src_vertex_id: 0,
            dst_vertex_id: 1,
            length_meters: 1609.344,
            highway_group: "primary".to_string(),
            road_class: CongestionRoadClass::Primary,
            posted_speed_mph: Some(40.0),
            imputed_speed_limit_mph: 40.0,
            geometry: LineString::from(vec![(-105.010, 40.000), (-104.991, 40.000)]),
        };

        // 1 point right on the midpoint (-105.0005, 40.000) with weight 3.0 (inside 0.5 mi)
        // 1 traffic control point ~110m north (-105.0005, 40.001) with weight 2.0 (inside 0.5 mi)
        // 1 point ~2.2km north (-105.0005, 40.020) with weight 100.0 (outside 0.5 mi)
        let rtree = RTree::bulk_load(vec![
            SpatialFeaturePoint {
                lon: -105.0005,
                lat: 40.000,
                kind: FeatureKind::Opportunity,
                weight: 3.0,
            },
            SpatialFeaturePoint {
                lon: -105.0005,
                lat: 40.001,
                kind: FeatureKind::TrafficControl,
                weight: 2.0,
            },
            SpatialFeaturePoint {
                lon: -105.0005,
                lat: 40.020,
                kind: FeatureKind::Opportunity,
                weight: 100.0,
            },
        ]);

        let (opp_den, tc_den) =
            compute_edge_poi_and_traffic_densities(&edge, &rtree, 0.5, ProjectionMode::Geodesic);

        // Analytical buffer area for R = 0.5 mi, L = 1.0 mi: pi * (0.5)^2 + 2 * 0.5 * 1.0 = pi/4 + 1.0
        let expected_area = std::f64::consts::PI * 0.25 + 1.0;
        let expected_opp = (3.0 / expected_area) as f32;
        let expected_tc = (2.0 / expected_area) as f32;

        assert!((opp_den - expected_opp).abs() < 1e-4);
        assert!((tc_den - expected_tc).abs() < 1e-4);
    }
}
