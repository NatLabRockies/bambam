use super::SourceFormat;
use bambam_core::util::polygonal_rtree::PolygonalRTree;
use csv::Reader;
use geo::{
    triangulate_delaunay::DelaunayTriangulationConfig, Area, BoundingRect, Contains,
    TriangulateDelaunay,
};
use itertools::Itertools;
use kdam::{term, tqdm, Bar, BarExt};
use rand;
use rand::prelude::*;
use rayon::prelude::*;
use routee_compass_core::{
    model::{
        map::{DistanceTolerance, MapModelConfig, NearestSearchResult, SpatialIndex},
        network::Vertex,
        unit::DistanceUnit,
    },
    util::fs::read_utils,
};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use wkt::{self, ToWkt};

/// reads in opportunity data from some long-formatted opportunity dataset and aggregates
/// it to some vertex dataset
pub fn run(
    vertices_compass_filename: &str,
    opportunities_filename: &str,
    output_filename: &str,
    source_format: &SourceFormat,
    // activity_categories: &[String],
) -> Result<(), String> {
    // load Compass Vertices, create spatial index
    let bar_builder = Bar::builder().desc("read vertices file");
    let vertices: Box<[Vertex]> = read_utils::from_csv(
        &Path::new(vertices_compass_filename),
        true,
        Some(bar_builder),
        None,
    )
    .map_err(|e| format!("{e}"))?;
    let spatial_index = Arc::new(SpatialIndex::new_vertex_oriented(
        &vertices,
        Some(uom::si::f64::Length::new::<uom::si::length::meter>(200.0)),
    ));

    // load opportunity data, build activity types lookup
    let opportunities: Vec<OppRow> =
        read_opportunity_rows_v2(opportunities_filename, source_format)?;
    let activity_types_lookup = source_format
        .activity_categories()
        .into_iter()
        .enumerate()
        .map(|(i, s)| (s, i))
        .collect::<HashMap<_, _>>();

    // find a VertexId for each opportunity via nearest neighbors search tree
    let nearest_bar = Arc::new(Mutex::new(
        Bar::builder()
            .desc("attach opportunities to graph")
            .total(opportunities.len())
            .build()
            .unwrap(),
    ));
    let nearest_results = opportunities
        .into_par_iter()
        .flat_map(
            |OppRow {
                 geometry,
                 index,
                 category,
                 count,
             }| {
                if let Ok(mut bar) = nearest_bar.clone().lock() {
                    let _ = bar.update(1);
                }
                match spatial_index.clone().nearest_graph_id(&geometry) {
                    Ok(NearestSearchResult::NearestVertex(vertex_id)) => {
                        Some((vertex_id, (geometry, index, category)))
                    }
                    _ => None,
                }
            },
        )
        .collect_vec_list();
    eprintln!();

    // group opportunities by nearest vertex id
    let group_iter = tqdm!(
        nearest_results.into_iter(),
        desc = "group opportunities by vertex id"
    );
    let grouped = group_iter.flatten().into_group_map();
    eprintln!();

    // aggregate long-format data to wide-format using the activity type lookup to
    // increment values at vector indices.
    term::init(false);
    term::hide_cursor().map_err(|e| format!("progress bar error: {e}"))?;
    let result_iter = tqdm!(
        vertices.iter(),
        desc = "aggregate opportunities",
        total = vertices.len(),
        position = 0
    );

    let mut act_bars = activity_types_lookup
        .iter()
        .map(|(act, index)| {
            Bar::builder()
                .position(*index as u16 + 1)
                .desc(act)
                .build()
                .unwrap()
        })
        .collect_vec();
    let result: Vec<Vec<u64>> = result_iter
        .map(|v| match grouped.get(&v.vertex_id) {
            None => Ok(vec![0; activity_types_lookup.len()]),
            Some(opps) => {
                let mut out_row = vec![0; activity_types_lookup.len()];
                for (_, _, cat) in opps.iter() {
                    match activity_types_lookup.get(cat) {
                        None => {}
                        Some(out_index) => {
                            let _ = act_bars[*out_index].update(1);
                            out_row[*out_index] += 1;
                        }
                    }
                    // let out_index = activity_types_lookup.get(cat).ok_or_else(|| {
                    //     format!(
                    //         "internal error: missing category index for opportunity category '{}'",
                    //         cat
                    //     )
                    // })?;
                    // let _ = act_bars[*out_index].update(1);
                    // out_row[*out_index] = out_row[*out_index] + 1;
                }
                Ok(out_row)
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    eprintln!();
    for _ in act_bars.iter() {
        eprintln!();
    }
    term::show_cursor().map_err(|e| format!("progress bar error: {e}"))?;

    // write opportunity vectors
    let opportunities_compass_file = Path::new(output_filename);
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_path(opportunities_compass_file)
        .map_err(|e| {
            let opp_file_str = opportunities_compass_file.to_string_lossy();
            format!("failure opening output file {opp_file_str}: {e}")
        })?;
    let n_output_rows = result.len();
    let write_iter = tqdm!(
        result.into_iter().enumerate(),
        desc = "writing opportunities.csv",
        total = n_output_rows
    );
    for (idx, row) in write_iter {
        let serialized = row.into_iter().map(|v| format!("{v}")).collect_vec();
        writer
            .write_record(&serialized)
            .map_err(|e| format!("failure writing CSV output row {idx}: {e}"))?;
    }
    eprintln!();

    Ok(())
}

pub struct OppRow {
    pub geometry: geo::Point<f32>,
    pub index: usize,
    pub category: String,
    pub count: u64,
}

pub fn read_opportunity_rows_v2(
    opportunities_filename: &str,
    source_format: &SourceFormat,
) -> Result<Vec<OppRow>, String> {
    let mut opps_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(opportunities_filename)
        .map_err(|e| format!("failed to load {opportunities_filename}: {e}"))?;
    let headers = build_header_lookup(&mut opps_reader)?;
    let bar = Arc::new(Mutex::new(
        Bar::builder()
            .desc("deserialize opportunities")
            .build()
            .map_err(|e| e.to_string())?,
    ));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let outer_errors = errors.clone();
    let result = opps_reader
        .records()
        .enumerate()
        .collect_vec()
        .into_par_iter()
        .flat_map(|(idx, row)| {
            let inner_bar_clone = bar.clone();
            let mut inner_bar = match handle_failure(inner_bar_clone.lock(), errors.clone()) {
                Some(b) => b,
                None => return vec![],
            };
            inner_bar.update(1);
            let record = match handle_failure(row, errors.clone()) {
                Some(r) => r,
                None => return vec![],
            };
            //
            let geometry_opt = match handle_failure(
                source_format.read_geometry(&record, &headers),
                errors.clone(),
            ) {
                Some(g_opt) => g_opt,
                None => return vec![],
            };
            let counts_by_category = match handle_failure(
                source_format.get_counts_by_category(&record, &headers),
                errors.clone(),
            ) {
                Some(cats) => cats,
                None => return vec![],
            };
            match geometry_opt {
                None => vec![],
                Some(geo::Geometry::Point(point)) => counts_by_category
                    .into_iter()
                    .map(|(act, cnt)| OppRow {
                        geometry: point,
                        index: idx,
                        category: act.clone(),
                        count: cnt,
                    })
                    .collect_vec(),
                Some(geo::Geometry::Polygon(polygon)) => {
                    downsample_polygon(&geo::Geometry::Polygon(polygon), &counts_by_category)
                        .expect("failed to downsample polygon")
                }
                Some(geo::Geometry::MultiPolygon(mp)) => {
                    downsample_polygon(&geo::Geometry::MultiPolygon(mp), &counts_by_category)
                        .expect("failed to downsample multipolygon")
                }
                Some(other) => panic!("unsupported geometry type: {}", other.to_wkt()),
            }
        })
        .collect_vec_list()
        .into_iter()
        .flatten()
        .collect_vec();
    eprintln!();

    let final_errors = match outer_errors.lock() {
        Err(e) => return Err(e.to_string()),
        Ok(final_errors) => final_errors,
    };
    // let final_errors = errors.clone().lock();
    if final_errors.is_empty() {
        Ok(result)
    } else {
        Err(final_errors.iter().join(","))
    }
}

fn handle_failure<T, E: ToString>(
    result: Result<T, E>,
    errors: Arc<Mutex<Vec<String>>>,
) -> Option<T> {
    match result.map_err(|e| e.to_string()) {
        Ok(t) => Some(t),
        Err(e) => {
            if let Ok(mut errs) = errors.clone().lock() {
                errs.push(e)
            }
            None
        }
    }
}

/// uniformly samples locations for each opportunity in the counts collection.
/// first triangulates the (multi)polygon
fn downsample_polygon(
    polygon: &geo::Geometry<f32>,
    counts: &HashMap<String, u64>,
) -> Result<Vec<OppRow>, String> {
    let triangles = match polygon {
        geo::Geometry::Polygon(g) => g
            .unconstrained_triangulation()
            .map_err(|e| format!("failure triangulating polygon: {e}")),
        geo::Geometry::MultiPolygon(g) => g
            .unconstrained_triangulation()
            .map_err(|e| format!("failure triangulating polygon: {e}")),
        _ => Err(format!(
            "cannot triangulate non-polygonal geometry: {}",
            polygon.to_wkt()
        )),
    }?;
    if triangles.is_empty() {
        return Err(format!(
            "triangulation of polygon produced no triangles: {}",
            polygon.to_wkt()
        ));
    }
    let weighted_triangles = triangles
        .into_iter()
        .map(|t| (t, t.unsigned_area()))
        .collect_vec();

    // for each activity count, sample a point to assign it
    let mut rng = rand::rng();
    let mut idx = 0;
    let result = counts
        .iter()
        .map(|(act, cnt)| {
            (0..*cnt as usize)
                .map(|_| {
                    let (triangle, _) = weighted_triangles
                        .choose_weighted(&mut rng, |t| t.1)
                        .map_err(|e| {
                            format!(
                                "failure sampling from {} triangles using weighted sampling algorithm: {}",
                                weighted_triangles.len(), e
                            )
                        })?;
                    let pt = sample_point_from_triangle(triangle, &mut rng);
                    let row = OppRow {
                        geometry: pt,
                        index: idx,
                        category: act.clone(),
                        count: 1,
                    };
                    idx += 1;
                    Ok(row)
                })
                .collect::<Result<Vec<OppRow>, String>>()
        })
        .collect::<Result<Vec<Vec<OppRow>>, String>>()?;
    Ok(result.into_iter().flatten().collect_vec())
}

/// samples a point from within a triangle using a barycentric coordinate representation and sampling
/// along vectors between point 1 and the other two points.
fn sample_point_from_triangle(t: &geo::Triangle<f32>, rng: &mut ThreadRng) -> geo::Point<f32> {
    let (mut r1, mut r2) = (rng.random::<f32>(), rng.random::<f32>());
    // ensure point will fall within the triangle
    if r1 + r2 > 1.0 {
        r1 = 1.0 - r1;
        r2 = 1.0 - r2;
    }
    let (p1, p2, p3) = (t.0, t.1, t.2);
    // apply vectors to p1 that stretch it randomly towards p2 + p3
    let t1 = geo::Point(p1);
    let t2 = geo::Point(p2 - p1) * r1;
    let t3 = geo::Point(p3 - p1) * r2;
    t1 + t2 + t3
}

pub fn build_header_lookup(reader: &mut Reader<File>) -> Result<HashMap<String, usize>, String> {
    // We nest this call in its own scope because of lifetimes.
    let headers = reader
        .headers()
        .map_err(|e| format!("failure retrieving headers: {e}"))?;
    let lookup: HashMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(idx, col)| (String::from(col), idx))
        .collect::<HashMap<_, _>>();

    Ok(lookup)
}
