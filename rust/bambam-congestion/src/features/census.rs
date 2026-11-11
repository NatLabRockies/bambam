use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufWriter, Read, Write},
    path::Path,
};

use bamcensus::app::{acs_tiger, lodes_tiger};
use bamcensus_acs::model::{AcsApiQueryParams, AcsGeoidQuery};
use bamcensus_core::model::identifier::{Geoid, HasGeoidString, StateCode};
use bamcensus_lehd::model::LodesDataset;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use geo::{Area, BoundingRect, Contains, Coord, Geometry, MultiPolygon, Point, Polygon, Rect};
use geozero::{geojson::GeoJsonString, wkt::Wkt as WktReader, ToGeo, ToWkt};
use itertools::Itertools;
use routee_compass_core::util::geo::PolygonalRTree;
use rstar::{RTree, RTreeObject, AABB};
use serde::Serialize;

use crate::{
    config::{
        BamcensusLodesConfig, BamcensusPopulationConfig, CensusSourceConfig, CensusStudyRegion,
        ProjectionMode,
    },
    conflate::{expand_bbox_by_meters, LocalMeterProjector},
    error::CongestionError,
    features::poi_traffic::lonlat_to_epsg3857,
    network::{open_buffered_reader, OpenNetworkEdge},
};

const METERS_PER_MILE: f64 = 1609.344;
/// Conversion factor from `per km^2` to `1,000 per sq mile` (`* 2.59 / 1000.0`),
/// matching `calibration_utils.py`.
const KM2_TO_THOUSAND_PER_SQ_MI: f64 = 2.59 / 1000.0;

/// A Census polygon (block group or tract) indexed in an `rstar::RTree`.
#[derive(Debug, Clone)]
pub struct CensusPolygonEntry {
    pub geoid: String,
    /// Population density in `1,000 persons / sq mi` (`pop_per_km2 * 2.59 / 1000.0`).
    pub pop_density_1000_per_sq_mi: f32,
    /// Employment density in `1,000 jobs / sq mi` (`emp_per_km2 * 2.59 / 1000.0`).
    pub emp_density_1000_per_sq_mi: f32,
    pub geometry: MultiPolygon<f64>,
    pub bbox: AABB<[f64; 2]>,
}

impl RTreeObject for CensusPolygonEntry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.bbox
    }
}

/// Builds an `RTree<CensusPolygonEntry>` from `CensusSourceConfig`, filtered to `study_bbox`.
pub fn build_census_rtree(
    census_cfg: Option<&CensusSourceConfig>,
    study_bbox: Rect<f64>,
    density_radius_miles: f64,
    projection_mode: ProjectionMode,
) -> Result<RTree<CensusPolygonEntry>, CongestionError> {
    let Some(cfg) = census_cfg else {
        return Ok(RTree::new());
    };

    let radius_m = density_radius_miles * METERS_PER_MILE;
    let search_bbox = expand_bbox_by_meters(study_bbox, radius_m * 2.0 + 1000.0);

    let entries = match cfg {
        CensusSourceConfig::Bamcensus {
            population_source,
            employment_source,
            save_csv_path,
        } => load_census_bamcensus(
            population_source,
            employment_source,
            save_csv_path.as_deref().map(Path::new),
            search_bbox,
            projection_mode,
        )?,
        CensusSourceConfig::Shapefile {
            path,
            pop_density_field,
            emp_density_field,
        } => load_census_shapefile(
            Path::new(path),
            pop_density_field,
            emp_density_field,
            search_bbox,
        )?,
        CensusSourceConfig::CsvPolygons {
            polygons_csv_path,
            geoid_column,
            wkt_column,
            population_column,
            employment_column,
            population_csv_path,
            population_csv_geoid_column,
            population_csv_value_column,
            employment_csv_path,
            employment_csv_geoid_column,
            employment_csv_value_column,
        } => load_census_csv_polygons(
            Path::new(polygons_csv_path),
            geoid_column,
            wkt_column,
            population_column.as_deref(),
            employment_column.as_deref(),
            population_csv_path.as_deref().map(Path::new),
            population_csv_geoid_column.as_deref(),
            population_csv_value_column.as_deref(),
            employment_csv_path.as_deref().map(Path::new),
            employment_csv_geoid_column.as_deref(),
            employment_csv_value_column.as_deref(),
            search_bbox,
            projection_mode,
        )?,
    };

    log::info!("Indexed {} Census polygons into R*-tree", entries.len());
    Ok(RTree::bulk_load(entries))
}

/// Computes the area-weighted `(pop_density_1000_per_sq_mi, emp_density_1000_per_sq_mi)`
/// within the `density_radius_miles` buffer around `edge`.
///
/// Never drops edges: if no Census polygon overlaps the edge's buffer, returns `(0.0, 0.0)`.
pub fn compute_edge_census_densities(
    edge: &OpenNetworkEdge,
    census_rtree: &RTree<CensusPolygonEntry>,
    density_radius_miles: f64,
) -> (f32, f32) {
    if census_rtree.size() == 0 {
        return (0.0, 0.0);
    }
    let Some(rect) = edge.geometry.bounding_rect() else {
        return (0.0, 0.0);
    };

    let radius_m = density_radius_miles * METERS_PER_MILE;
    let expanded = expand_bbox_by_meters(rect, radius_m);
    let env = AABB::from_corners(
        [expanded.min().x, expanded.min().y],
        [expanded.max().x, expanded.max().y],
    );

    let candidates: Vec<&CensusPolygonEntry> =
        census_rtree.locate_in_envelope_intersecting(&env).collect();
    if candidates.is_empty() {
        return (0.0, 0.0);
    }

    // Fast path: if only 1 candidate polygon's bbox intersects the buffer and contains the center
    let center = rect.center();
    if candidates.len() == 1 && candidates[0].geometry.contains(&Point::from(center)) {
        return (
            candidates[0].pop_density_1000_per_sq_mi,
            candidates[0].emp_density_1000_per_sq_mi,
        );
    }

    // General path: sample 13 quadrature points across the 0.5-mile buffer capsule around the edge
    let sample_pts = build_buffer_quadrature_points(&edge.geometry, center, radius_m);
    let mut sum_pop = 0.0_f64;
    let mut sum_emp = 0.0_f64;
    let mut hits = 0usize;

    for pt in &sample_pts {
        let g_pt = Point::from(*pt);
        for cand in &candidates {
            let [min_x, min_y] = cand.bbox.lower();
            let [max_x, max_y] = cand.bbox.upper();
            if pt.x >= min_x
                && pt.x <= max_x
                && pt.y >= min_y
                && pt.y <= max_y
                && cand.geometry.contains(&g_pt)
            {
                sum_pop += cand.pop_density_1000_per_sq_mi as f64;
                sum_emp += cand.emp_density_1000_per_sq_mi as f64;
                hits += 1;
                break;
            }
        }
    }

    if hits == 0 {
        return (0.0, 0.0);
    }
    let inv = 1.0 / (hits as f64);
    ((sum_pop * inv) as f32, (sum_emp * inv) as f32)
}

/// Generates 13 deterministic quadrature points in WGS84 `(lon, lat)` distributed across
/// the buffer capsule of radius `radius_m` around `ls`.
fn build_buffer_quadrature_points(
    ls: &geo::LineString<f64>,
    center: Coord<f64>,
    radius_m: f64,
) -> [Coord<f64>; 13] {
    let first = ls.0.first().copied().unwrap_or(center);
    let last = ls.0.last().copied().unwrap_or(center);
    let mid = ls.0.get(ls.0.len() / 2).copied().unwrap_or(center);

    let cos_lat = center.y.to_radians().cos().abs().max(0.01);
    let deg_lat_per_m = 1.0 / 110_574.27;
    let deg_lon_per_m = 1.0 / (111_319.49 * cos_lat);

    let r1 = 0.45 * radius_m;
    let r2 = 0.85 * radius_m;
    let d1_lon = r1 * deg_lon_per_m;
    let d1_lat = r1 * deg_lat_per_m;
    let d2_lon = r2 * deg_lon_per_m;
    let diag = std::f64::consts::FRAC_1_SQRT_2 * r2;
    let dd_lon = diag * deg_lon_per_m;
    let dd_lat = diag * deg_lat_per_m;

    [
        mid,
        first,
        last,
        Coord {
            x: mid.x + d1_lon,
            y: mid.y,
        },
        Coord {
            x: mid.x - d1_lon,
            y: mid.y,
        },
        Coord {
            x: mid.x,
            y: mid.y + d1_lat,
        },
        Coord {
            x: mid.x,
            y: mid.y - d1_lat,
        },
        Coord {
            x: mid.x + d2_lon,
            y: mid.y,
        },
        Coord {
            x: mid.x - d2_lon,
            y: mid.y,
        },
        Coord {
            x: mid.x + dd_lon,
            y: mid.y + dd_lat,
        },
        Coord {
            x: mid.x - dd_lon,
            y: mid.y + dd_lat,
        },
        Coord {
            x: mid.x + dd_lon,
            y: mid.y - dd_lat,
        },
        Coord {
            x: mid.x - dd_lon,
            y: mid.y - dd_lat,
        },
    ]
}

/// Downloads US Census ACS population counts, LEHD LODES employment counts, and TIGER/Line
/// polygons via `bamcensus` (`acs_tiger::run_batch` and `lodes_tiger::run`), filters to polygons
/// intersecting `search_bbox`, optionally writes the joined table (`geoid,population,jobs,geometry`)
/// to `save_csv_path`, and converts counts into `pop_density_1000_per_sq_mi` and
/// `emp_density_1000_per_sq_mi` (`count / area_km2 * 2.59 / 1000.0`).
///
/// Requires the user's Census API key in `CENSUS_API_KEY` (or `CENSUS_API_TOKEN`) in the environment
/// (or set on `population_source.api_token`).
pub fn load_census_bamcensus(
    population_source: &BamcensusPopulationConfig,
    employment_source: &BamcensusLodesConfig,
    save_csv_path: Option<&Path>,
    search_bbox: Rect<f64>,
    projection_mode: ProjectionMode,
) -> Result<Vec<CensusPolygonEntry>, CongestionError> {
    let api_token = population_source.resolve_api_token()?;
    let state_geoids = resolve_bamcensus_state_geoids(search_bbox)?;
    if state_geoids.is_empty() {
        return Err(CongestionError::SpatialFeature(format!(
            "no US states intersect the study bounding box {search_bbox:?}"
        )));
    }

    let lodes_geoids = match &employment_source.study_region {
        Some(CensusStudyRegion::National) => StateCode::ALL
            .iter()
            .map(|s| Geoid::try_from(s.to_fips_string().as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CongestionError::SpatialFeature)?,
        Some(CensusStudyRegion::Census { geoids }) => geoids
            .iter()
            .map(|s| Geoid::try_from(s.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CongestionError::SpatialFeature)?,
        None => state_geoids.clone(),
    };

    let acs_categories = population_source
        .acs_categories
        .clone()
        .unwrap_or_else(|| vec!["B01001_001E".to_string()]);

    let acs_queries = state_geoids
        .iter()
        .map(|state_geoid| {
            let geoid_query =
                AcsGeoidQuery::new(Some(state_geoid.clone()), population_source.acs_resolution)
                    .map_err(CongestionError::SpatialFeature)?;
            Ok(AcsApiQueryParams::new(
                None,
                population_source.acs_year,
                population_source.acs_type,
                acs_categories.clone(),
                geoid_query,
                Some(api_token.clone()),
            ))
        })
        .collect::<Result<Vec<_>, CongestionError>>()?;

    let lodes_dataset = LodesDataset::WAC {
        edition: employment_source.edition,
        job_type: employment_source.job_type,
        segment: employment_source.segment,
        year: employment_source.year,
    };
    let wac_segments = employment_source.resolved_wac_segments();
    let lodes_granularity = employment_source
        .data_granularity
        .or(population_source.acs_resolution);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            CongestionError::SpatialFeature(format!(
                "failure creating async tokio runtime for bamcensus: {e}"
            ))
        })?;

    let (acs_res, lodes_res) = runtime.block_on(async {
        let acs = acs_tiger::run_batch(&acs_queries).await.map_err(|e| {
            CongestionError::SpatialFeature(format!("failure downloading ACS data: {e}"))
        })?;
        let lodes = lodes_tiger::run(
            &lodes_geoids,
            &lodes_granularity,
            &wac_segments,
            &lodes_dataset,
            employment_source.fallback.0,
        )
        .await
        .map_err(|e| {
            CongestionError::SpatialFeature(format!("failure downloading LODES data: {e}"))
        })?;
        Ok::<_, CongestionError>((acs, lodes))
    })?;

    if !acs_res.join_errors.is_empty() || !acs_res.tiger_errors.is_empty() {
        return Err(CongestionError::SpatialFeature(format!(
            "failures downloading ACS data.\nTIGER ERRORS (top 5):\n  {}\nJOIN ERRORS (top 5):\n  {}",
            acs_res.tiger_errors.iter().take(5).join("\n  "),
            acs_res.join_errors.iter().take(5).join("\n  ")
        )));
    }
    if !lodes_res.join_errors.is_empty() || !lodes_res.tiger_errors.is_empty() {
        return Err(CongestionError::SpatialFeature(format!(
            "failures downloading LODES data.\nTIGER ERRORS (top 5):\n  {}\nJOIN ERRORS (top 5):\n  {}",
            lodes_res.tiger_errors.iter().take(5).join("\n  "),
            lodes_res.join_errors.iter().take(5).join("\n  ")
        )));
    }

    // Aggregate population counts and geometries by normalized GEOID
    let mut geom_by_geoid: HashMap<String, MultiPolygon<f64>> = HashMap::new();
    let mut pop_by_geoid: HashMap<String, f64> = HashMap::new();
    let mut emp_by_geoid: HashMap<String, f64> = HashMap::new();

    for row in acs_res.join_dataset {
        let geoid = normalize_geoid(&row.geoid.geoid_string());
        let val = row
            .acs_value
            .as_f64_safe()
            .map_err(CongestionError::SpatialFeature)?;
        if val.is_finite() {
            *pop_by_geoid.entry(geoid.clone()).or_insert(0.0) += val;
        }
        if let std::collections::hash_map::Entry::Vacant(e) = geom_by_geoid.entry(geoid) {
            if let Some(mp) = geometry_to_multipolygon(row.geometry) {
                e.insert(mp);
            }
        }
    }

    for row in lodes_res.join_dataset {
        let geoid = normalize_geoid(&row.geoid.geoid_string());
        let val = row.value.value;
        if val.is_finite() {
            *emp_by_geoid.entry(geoid.clone()).or_insert(0.0) += val;
        }
        if let std::collections::hash_map::Entry::Vacant(e) = geom_by_geoid.entry(geoid) {
            if let Some(mp) = geometry_to_multipolygon(row.geometry) {
                e.insert(mp);
            }
        }
    }

    let mut out = Vec::new();
    let mut csv_rows: Vec<SavedBamcensusRow> = Vec::new();
    let mut sorted_geoids: Vec<String> = geom_by_geoid.keys().cloned().collect();
    sorted_geoids.sort();

    for geoid in sorted_geoids {
        let Some(mp) = geom_by_geoid.remove(&geoid) else {
            continue;
        };
        let Some(rect) = mp.bounding_rect() else {
            continue;
        };
        if rect.max().x < search_bbox.min().x
            || rect.min().x > search_bbox.max().x
            || rect.max().y < search_bbox.min().y
            || rect.min().y > search_bbox.max().y
        {
            continue;
        }
        let area_km2 = compute_multipolygon_area_km2(&mp, rect.center(), projection_mode);
        if area_km2 <= 1e-9 {
            continue;
        }
        let pop_count = pop_by_geoid.get(&geoid).copied().unwrap_or(0.0);
        let emp_count = emp_by_geoid.get(&geoid).copied().unwrap_or(0.0);
        let pop_den_km2 = pop_count / area_km2;
        let emp_den_km2 = emp_count / area_km2;

        if save_csv_path.is_some() {
            if let Ok(wkt_geom) = Geometry::MultiPolygon(mp.clone()).to_wkt() {
                csv_rows.push(SavedBamcensusRow {
                    geoid: geoid.clone(),
                    population: pop_count,
                    jobs: emp_count,
                    geometry: wkt_geom,
                });
            }
        }

        let bbox = AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]);
        out.push(CensusPolygonEntry {
            geoid,
            pop_density_1000_per_sq_mi: (pop_den_km2 * KM2_TO_THOUSAND_PER_SQ_MI) as f32,
            emp_density_1000_per_sq_mi: (emp_den_km2 * KM2_TO_THOUSAND_PER_SQ_MI) as f32,
            geometry: mp,
            bbox,
        });
    }

    if let Some(save_path) = save_csv_path {
        write_bamcensus_csv(save_path, &csv_rows)?;
    }

    Ok(out)
}

#[derive(Debug, Serialize)]
struct SavedBamcensusRow {
    geoid: String,
    population: f64,
    jobs: f64,
    geometry: String,
}

/// Writes the downloaded `bamcensus` polygon dataset (`geoid,population,jobs,geometry`) to a
/// CSV or `.csv.gz` file on disk so it can be reused offline via `CensusSourceConfig::CsvPolygons`.
fn write_bamcensus_csv(path: &Path, rows: &[SavedBamcensusRow]) -> Result<(), CongestionError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|source| CongestionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let file = File::create(path).map_err(|source| CongestionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let is_gz = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));
    let writer: Box<dyn Write> = if is_gz {
        Box::new(GzEncoder::new(BufWriter::new(file), Compression::default()))
    } else {
        Box::new(BufWriter::new(file))
    };

    let mut csv_writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(writer);
    for row in rows {
        csv_writer
            .serialize(row)
            .map_err(|e| CongestionError::Csv {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?;
    }
    csv_writer.flush().map_err(|source| CongestionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    log::info!(
        "Saved {} bamcensus polygons (geoid, population, jobs, geometry) to '{}'",
        rows.len(),
        path.display()
    );
    Ok(())
}

/// Converts a [`geo::Geometry`] into a [`MultiPolygon`] if it is a `Polygon` or `MultiPolygon`.
fn geometry_to_multipolygon(geom: Geometry<f64>) -> Option<MultiPolygon<f64>> {
    match geom {
        Geometry::Polygon(p) => Some(MultiPolygon(vec![p])),
        Geometry::MultiPolygon(mp) => Some(mp),
        _ => None,
    }
}

/// Loads the embedded TIGER/Line US states polygonal R*-tree and returns the state [`Geoid`]s
/// intersecting `search_bbox`.
fn resolve_bamcensus_state_geoids(search_bbox: Rect<f64>) -> Result<Vec<Geoid>, CongestionError> {
    let state_file = include_bytes!(
        "../../../bambam/src/model/input_plugin/population/tl_2023_us_state_in_tiger_lines.geojson.gz"
    );
    let mut decoder = GzDecoder::new(&state_file[..]);
    let mut buf = String::new();
    decoder.read_to_string(&mut buf).map_err(|e| {
        CongestionError::SpatialFeature(format!(
            "failure reading tl_2023_us_state_in_tiger_lines.geojson.gz into memory: {e}"
        ))
    })?;
    let json: serde_json::Value = serde_json::from_str(&buf).map_err(|e| {
        CongestionError::SpatialFeature(format!(
            "failure reading tl_2023_us_state_in_tiger_lines.geojson.gz as GeoJSON: {e}"
        ))
    })?;
    let features = json["features"].as_array().ok_or_else(|| {
        CongestionError::SpatialFeature(
            "state reference file tl_2023_us_state_in_tiger_lines.geojson.gz expected to be a FeatureCollection"
                .to_string(),
        )
    })?;

    let mut tree_nodes = Vec::with_capacity(features.len());
    for feature in features {
        let geoid_str = feature["properties"]["GEOID"].as_str().ok_or_else(|| {
            CongestionError::SpatialFeature("no GEOID in state feature".to_string())
        })?;
        let geoid: Geoid = geoid_str.try_into().map_err(|e| {
            CongestionError::SpatialFeature(format!("failure decoding state GEOID: {e}"))
        })?;
        let geometry_json = serde_json::to_string(&feature["geometry"]).map_err(|e| {
            CongestionError::SpatialFeature(format!(
                "failure serializing geometry for state GEOID {geoid}: {e}"
            ))
        })?;
        let geometry: Geometry = GeoJsonString(geometry_json).to_geo().map_err(|e| {
            CongestionError::SpatialFeature(format!(
                "failure decoding GeoJSON geometry for state GEOID {geoid}: {e}"
            ))
        })?;
        tree_nodes.push((geometry, geoid));
    }

    let state_tree = PolygonalRTree::new(tree_nodes).map_err(CongestionError::SpatialFeature)?;
    let query_geom = Geometry::Polygon(search_bbox.to_polygon());
    let states: Vec<Geoid> = state_tree
        .intersection(&query_geom)
        .map_err(CongestionError::SpatialFeature)?
        .map(|node| node.data.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    Ok(states)
}

/// Reads a prepared Census shapefile containing population density (`pop_field`, per $\text{km}^2$)
/// and employment density (`emp_field`, per $\text{km}^2$) attributes, filters polygons to those
/// intersecting `search_bbox`, and converts densities to `1,000 per sq mile`.
fn load_census_shapefile(
    path: &Path,
    pop_field: &str,
    emp_field: &str,
    search_bbox: Rect<f64>,
) -> Result<Vec<CensusPolygonEntry>, CongestionError> {
    let rows = shapefile::read(path).map_err(|e| {
        CongestionError::SpatialFeature(format!(
            "failed reading Census shapefile '{}': {e}",
            path.display()
        ))
    })?;

    let mut out = Vec::new();
    for (idx, (shape, record)) in rows.into_iter().enumerate() {
        let mp: MultiPolygon<f64> = match shape {
            shapefile::Shape::Polygon(p) => p.into(),
            shapefile::Shape::PolygonM(p) => p.into(),
            shapefile::Shape::PolygonZ(p) => p.into(),
            _ => continue,
        };
        let Some(rect) = mp.bounding_rect() else {
            continue;
        };
        if rect.max().x < search_bbox.min().x
            || rect.min().x > search_bbox.max().x
            || rect.max().y < search_bbox.min().y
            || rect.min().y > search_bbox.max().y
        {
            continue;
        }

        let pop_den_km2 = extract_dbf_numeric(&record, pop_field).unwrap_or(0.0);
        let emp_den_km2 = extract_dbf_numeric(&record, emp_field).unwrap_or(0.0);

        let bbox = AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]);
        out.push(CensusPolygonEntry {
            geoid: idx.to_string(),
            pop_density_1000_per_sq_mi: (pop_den_km2 * KM2_TO_THOUSAND_PER_SQ_MI) as f32,
            emp_density_1000_per_sq_mi: (emp_den_km2 * KM2_TO_THOUSAND_PER_SQ_MI) as f32,
            geometry: mp,
            bbox,
        });
    }
    Ok(out)
}

/// Extracts a numeric (`f64`) attribute value from a dBase shapefile record by field name.
fn extract_dbf_numeric(record: &shapefile::dbase::Record, field_name: &str) -> Option<f64> {
    match record.get(field_name)? {
        shapefile::dbase::FieldValue::Numeric(Some(v)) => Some(*v),
        shapefile::dbase::FieldValue::Float(Some(v)) => Some(*v as f64),
        shapefile::dbase::FieldValue::Double(v) => Some(*v),
        shapefile::dbase::FieldValue::Integer(v) => Some(*v as f64),
        shapefile::dbase::FieldValue::Character(Some(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Loads Census WKT polygons from a CSV file (such as `tiger-lines/tracts.csv`), joins inline or
/// external population and employment count tables by normalized GEOID, computes polygon area in
/// $\text{km}^2$ under `projection_mode`, and converts counts into `1,000 persons/jobs per sq mile`.
#[allow(clippy::too_many_arguments)]
fn load_census_csv_polygons(
    polygons_csv_path: &Path,
    geoid_column: &str,
    wkt_column: &str,
    population_column: Option<&str>,
    employment_column: Option<&str>,
    population_csv_path: Option<&Path>,
    population_csv_geoid_column: Option<&str>,
    population_csv_value_column: Option<&str>,
    employment_csv_path: Option<&Path>,
    employment_csv_geoid_column: Option<&str>,
    employment_csv_value_column: Option<&str>,
    search_bbox: Rect<f64>,
    projection_mode: ProjectionMode,
) -> Result<Vec<CensusPolygonEntry>, CongestionError> {
    let ext_pop_lookup = if let (Some(p), Some(g_col), Some(v_col)) = (
        population_csv_path,
        population_csv_geoid_column,
        population_csv_value_column,
    ) {
        Some(load_geoid_value_lookup(p, g_col, v_col)?)
    } else {
        None
    };

    let ext_emp_lookup = if let (Some(p), Some(g_col), Some(v_col)) = (
        employment_csv_path,
        employment_csv_geoid_column,
        employment_csv_value_column,
    ) {
        Some(load_geoid_value_lookup(p, g_col, v_col)?)
    } else {
        None
    };

    let reader = open_buffered_reader(polygons_csv_path)?;
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let headers = csv_reader
        .headers()
        .map_err(|e| CongestionError::Csv {
            path: polygons_csv_path.to_path_buf(),
            message: e.to_string(),
        })?
        .clone();
    let header_map: HashMap<&str, usize> =
        headers.iter().enumerate().map(|(i, h)| (h, i)).collect();

    let geoid_idx = *header_map.get(geoid_column).ok_or_else(|| {
        CongestionError::SpatialFeature(format!(
            "Census CSV '{}' missing column '{}'",
            polygons_csv_path.display(),
            geoid_column
        ))
    })?;
    let wkt_idx = *header_map.get(wkt_column).ok_or_else(|| {
        CongestionError::SpatialFeature(format!(
            "Census CSV '{}' missing column '{}'",
            polygons_csv_path.display(),
            wkt_column
        ))
    })?;
    let pop_col_idx = population_column.and_then(|c| header_map.get(c).copied());
    let emp_col_idx = employment_column.and_then(|c| header_map.get(c).copied());

    let mut out = Vec::new();
    for rec_res in csv_reader.records() {
        let rec = rec_res.map_err(|e| CongestionError::Csv {
            path: polygons_csv_path.to_path_buf(),
            message: e.to_string(),
        })?;
        let Some(raw_geoid) = rec.get(geoid_idx) else {
            continue;
        };
        let geoid = normalize_geoid(raw_geoid);
        let Some(wkt_str) = rec.get(wkt_idx) else {
            continue;
        };
        let Ok(geom) = WktReader(wkt_str.trim()).to_geo() else {
            continue;
        };
        let mp: MultiPolygon<f64> = match geom {
            Geometry::Polygon(p) => MultiPolygon(vec![p]),
            Geometry::MultiPolygon(mp) => mp,
            _ => continue,
        };
        let Some(rect) = mp.bounding_rect() else {
            continue;
        };
        if rect.max().x < search_bbox.min().x
            || rect.min().x > search_bbox.max().x
            || rect.max().y < search_bbox.min().y
            || rect.min().y > search_bbox.max().y
        {
            continue;
        }

        let area_km2 = compute_multipolygon_area_km2(&mp, rect.center(), projection_mode);
        if area_km2 <= 1e-9 {
            continue;
        }

        let pop_count = pop_col_idx
            .and_then(|i| rec.get(i))
            .and_then(|s| s.trim().parse::<f64>().ok())
            .or_else(|| ext_pop_lookup.as_ref().and_then(|m| m.get(&geoid).copied()))
            .unwrap_or(0.0);

        let emp_count = emp_col_idx
            .and_then(|i| rec.get(i))
            .and_then(|s| s.trim().parse::<f64>().ok())
            .or_else(|| ext_emp_lookup.as_ref().and_then(|m| m.get(&geoid).copied()))
            .unwrap_or(0.0);

        let pop_den_km2 = pop_count / area_km2;
        let emp_den_km2 = emp_count / area_km2;

        let bbox = AABB::from_corners([rect.min().x, rect.min().y], [rect.max().x, rect.max().y]);
        out.push(CensusPolygonEntry {
            geoid,
            pop_density_1000_per_sq_mi: (pop_den_km2 * KM2_TO_THOUSAND_PER_SQ_MI) as f32,
            emp_density_1000_per_sq_mi: (emp_den_km2 * KM2_TO_THOUSAND_PER_SQ_MI) as f32,
            geometry: mp,
            bbox,
        });
    }

    Ok(out)
}

/// Computes the surface area of a [`MultiPolygon`] in $\text{km}^2$ using either a local
/// geodesic meter tangent-plane projection (`ProjectionMode::Geodesic`) or planar Web Mercator
/// (`ProjectionMode::LegacyEpsg3857`).
fn compute_multipolygon_area_km2(
    mp: &MultiPolygon<f64>,
    center: Coord<f64>,
    projection_mode: ProjectionMode,
) -> f64 {
    let mut area_m2 = 0.0;
    match projection_mode {
        ProjectionMode::Geodesic => {
            let proj = LocalMeterProjector::new(center.x, center.y);
            for poly in &mp.0 {
                let ext = proj.project_linestring(poly.exterior());
                let ints: Vec<_> = poly
                    .interiors()
                    .iter()
                    .map(|ls| proj.project_linestring(ls))
                    .collect();
                let p_m = Polygon::new(ext, ints);
                area_m2 += p_m.unsigned_area();
            }
        }
        ProjectionMode::LegacyEpsg3857 => {
            for poly in &mp.0 {
                let ext = geo::LineString(
                    poly.exterior()
                        .0
                        .iter()
                        .map(|c| lonlat_to_epsg3857(*c))
                        .collect(),
                );
                let ints: Vec<_> = poly
                    .interiors()
                    .iter()
                    .map(|ls| {
                        geo::LineString(ls.0.iter().map(|c| lonlat_to_epsg3857(*c)).collect())
                    })
                    .collect();
                let p_3857 = Polygon::new(ext, ints);
                area_m2 += p_3857.unsigned_area();
            }
        }
    }
    area_m2 * 1e-6
}

/// Reads a CSV (or `.csv.gz`) file and builds a lookup map from normalized Census GEOID
/// (`geoid_col`) to summed numeric count (`val_col`).
fn load_geoid_value_lookup(
    path: &Path,
    geoid_col: &str,
    val_col: &str,
) -> Result<HashMap<String, f64>, CongestionError> {
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
    let header_map: HashMap<&str, usize> =
        headers.iter().enumerate().map(|(i, h)| (h, i)).collect();
    let g_idx = *header_map.get(geoid_col).ok_or_else(|| {
        CongestionError::SpatialFeature(format!(
            "CSV '{}' missing GEOID column '{}'",
            path.display(),
            geoid_col
        ))
    })?;
    let v_idx = *header_map.get(val_col).ok_or_else(|| {
        CongestionError::SpatialFeature(format!(
            "CSV '{}' missing value column '{}'",
            path.display(),
            val_col
        ))
    })?;

    let mut map: HashMap<String, f64> = HashMap::new();
    for rec_res in csv_reader.records() {
        let rec = rec_res.map_err(|e| CongestionError::Csv {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        let Some(raw_g) = rec.get(g_idx) else {
            continue;
        };
        let Some(raw_v) = rec.get(v_idx) else {
            continue;
        };
        if let Ok(val) = raw_v.trim().parse::<f64>() {
            if val.is_finite() {
                let norm_g = normalize_geoid(raw_g);
                *map.entry(norm_g).or_insert(0.0) += val;
            }
        }
    }
    Ok(map)
}

/// Strips Census `AFFGEOID` prefixes like `"1400000US"` or `"1500000US"` so tract/block-group
/// GEOIDs match cleanly between ACS CSVs, LODES CSVs, and TIGER shapefiles.
pub fn normalize_geoid(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((_, suffix)) = trimmed.split_once("US") {
        suffix.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::CongestionRoadClass;
    use geo::LineString;

    #[test]
    fn test_normalize_geoid_and_census_lookup() {
        assert_eq!(normalize_geoid("1500000US080130122031"), "080130122031");
        assert_eq!(normalize_geoid("080130122031"), "080130122031");

        // Build two adjacent Census block group polygons split at lon = -105.000:
        // West BG (-105.02 to -105.00): pop_den = 2.0, emp_den = 1.0
        // East BG (-105.00 to -104.98): pop_den = 6.0, emp_den = 5.0
        let west_poly = Polygon::new(
            LineString::from(vec![
                (-105.02, 39.98),
                (-105.00, 39.98),
                (-105.00, 40.02),
                (-105.02, 40.02),
                (-105.02, 39.98),
            ]),
            vec![],
        );
        let east_poly = Polygon::new(
            LineString::from(vec![
                (-105.00, 39.98),
                (-104.98, 39.98),
                (-104.98, 40.02),
                (-105.00, 40.02),
                (-105.00, 39.98),
            ]),
            vec![],
        );

        let rtree = RTree::bulk_load(vec![
            CensusPolygonEntry {
                geoid: "west".to_string(),
                pop_density_1000_per_sq_mi: 2.0,
                emp_density_1000_per_sq_mi: 1.0,
                geometry: MultiPolygon(vec![west_poly]),
                bbox: AABB::from_corners([-105.02, 39.98], [-105.00, 40.02]),
            },
            CensusPolygonEntry {
                geoid: "east".to_string(),
                pop_density_1000_per_sq_mi: 6.0,
                emp_density_1000_per_sq_mi: 5.0,
                geometry: MultiPolygon(vec![east_poly]),
                bbox: AABB::from_corners([-105.00, 39.98], [-104.98, 40.02]),
            },
        ]);

        // 1. Edge completely inside West BG
        let west_edge = OpenNetworkEdge {
            edge_id: 0,
            source_id: "w".to_string(),
            src_vertex_id: 0,
            dst_vertex_id: 1,
            length_meters: 100.0,
            highway_group: "primary".to_string(),
            road_class: CongestionRoadClass::Primary,
            posted_speed_mph: Some(35.0),
            imputed_speed_limit_mph: 35.0,
            geometry: LineString::from(vec![(-105.015, 40.000), (-105.014, 40.000)]),
        };
        let (pop_w, emp_w) = compute_edge_census_densities(&west_edge, &rtree, 0.5);
        assert!((pop_w - 2.0).abs() < 1e-5);
        assert!((emp_w - 1.0).abs() < 1e-5);

        // 2. Edge straddling the boundary at -105.000 -> blended density between (2,1) and (6,5)
        let border_edge = OpenNetworkEdge {
            geometry: LineString::from(vec![(-105.001, 40.000), (-104.999, 40.000)]),
            ..west_edge.clone()
        };
        let (pop_b, emp_b) = compute_edge_census_densities(&border_edge, &rtree, 0.5);
        assert!(
            pop_b > 2.5 && pop_b < 5.5,
            "expected blended pop_den, got {pop_b}"
        );
        assert!(
            emp_b > 1.5 && emp_b < 4.5,
            "expected blended emp_den, got {emp_b}"
        );

        // 3. Edge over water far from any Census polygon -> returns (0.0, 0.0) without dropping edge
        let water_edge = OpenNetworkEdge {
            geometry: LineString::from(vec![(-106.000, 40.000), (-105.999, 40.000)]),
            ..west_edge
        };
        let (pop_zero, emp_zero) = compute_edge_census_densities(&water_edge, &rtree, 0.5);
        assert_eq!(pop_zero, 0.0);
        assert_eq!(emp_zero, 0.0);
    }

    #[test]
    fn test_bamcensus_state_geoids_and_config_deserialization() {
        // Verify that a Boulder bounding box resolves Colorado (FIPS 08) from the embedded TIGER state tree
        let boulder_bbox = Rect::new(
            Coord {
                x: -105.30,
                y: 39.98,
            },
            Coord {
                x: -105.20,
                y: 40.05,
            },
        );
        let states = resolve_bamcensus_state_geoids(boulder_bbox).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].geoid_string(), "08");

        // Verify TOML deserialization of CensusSourceConfig::Bamcensus matching bambam.toml conventions
        let toml_str = r#"
type = "bamcensus"

[population_source]
type = "acs"
acs_type = "five_year"
acs_year = 2022
acs_resolution = "census_tract"
acs_categories = ["B01001_001E"]

[employment_source]
type = "lodes"
study_region = { type = "census", geoids = ["08013"] }
data_granularity = "census_tract"
edition = "LODES8"
job_type = "JT00"
segment = "S000"
year = 2022
"#;
        let cfg: CensusSourceConfig = toml::from_str(toml_str).unwrap();
        match cfg {
            CensusSourceConfig::Bamcensus {
                population_source,
                employment_source,
                save_csv_path,
            } => {
                assert_eq!(population_source.acs_year, 2022);
                assert_eq!(employment_source.year, 2022);
                assert!(save_csv_path.is_none());
                assert_eq!(
                    employment_source.resolved_wac_segments(),
                    vec![bamcensus_lehd::model::WacSegment::C000]
                );
            }
            other => panic!("expected Bamcensus variant, got {other:?}"),
        }
    }
}
