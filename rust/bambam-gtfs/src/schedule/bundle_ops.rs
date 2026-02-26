use chrono::{Duration, NaiveDate, NaiveDateTime};
use csv::QuoteStyle;
use flate2::{write::GzEncoder, Compression};
use geo::{Contains, Geometry, LineString, Point};
use gtfs_structures::{Gtfs, Stop, StopTime};
use itertools::Itertools;
use kdam::{Bar, BarBuilder, BarExt};
use rayon::prelude::*;
use routee_compass_core::model::{
    map::{NearestSearchResult, SpatialIndex},
    network::{EdgeConfig, EdgeId, VertexId},
};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};
use uom::si::f64::Length;
use wkt::ToWkt;

use super::{GtfsBundle, GtfsEdge};
use crate::schedule::{
    batch_processing_error,
    date::DateMapping,
    distance_calculation_policy::{compute_haversine, DistanceCalculationPolicy},
    fq_ops,
    fq_schedule_row::FullyQualifiedScheduleRow,
    schedule_error::ScheduleError,
    DateMappingPolicy, MissingStopLocationPolicy, ScheduleRow, SortedTrip,
};

/// API for running batch or single bundle processing. configures the run of the GTFS import.
#[derive(Clone)]
pub struct ProcessBundlesConfig {
    /// lower value of date range for collecting a schedule for route planning
    pub start_date: String,
    /// upper value of date range for collecting a schedule for route planning
    pub end_date: String,
    /// offset for edge list identifier, can be zero or (last edge list id + 1)
    pub starting_edge_list_id: usize,
    /// used for map matching into the Compass graph.
    pub spatial_index: Arc<SpatialIndex>,
    /// app logic applied when a missing stop is encountered
    pub missing_stop_location_policy: MissingStopLocationPolicy,
    /// app logic applied to compute edge distances
    pub distance_calculation_policy: DistanceCalculationPolicy,
    /// app logic applied when filtering/mapping by date and time
    pub date_mapping_policy: DateMappingPolicy,
    /// optional boundary for including GTFS archives. if included, filters archives
    /// to those that intersect with the area within the provided extent.
    pub extent: Option<Geometry>,
    /// directory to write outputs
    pub output_directory: String,
    /// if true, allow overwriting files in output directory
    pub overwrite: bool,
}

/// multithreaded GTFS processing.
///
/// # Arguments
///
/// * `bundle_directory_path` - location of zipped GTFS archives
/// * `parallelism` - threads dedicated to GTFS import
/// * `conf` - configuration for processing, see for options
/// * `ignore_bad_gtfs` - if true, any failed processing does not terminate import and
///   remaining archives are processed into edge list outputs. errors are logged.
///
pub fn batch_process(
    bundle_directory_path: &Path,
    parallelism: usize,
    conf: Arc<ProcessBundlesConfig>,
    ignore_bad_gtfs: bool,
) -> Result<(), ScheduleError> {
    let archive_paths = bundle_directory_path
        .read_dir()
        .map_err(|e| ScheduleError::GtfsApp(format!("failure reading directory: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ScheduleError::GtfsApp(format!("failure reading directory: {e}")))?;
    let chunk_size = archive_paths.len() / std::cmp::max(1, parallelism);

    // a progress bar shared across threads
    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .desc("batch GTFS processing")
            .total(archive_paths.len())
            .animation("fillup")
            .build()
            .map_err(|e| ScheduleError::Internal(format!("failure building progress bar: {e}")))?,
    ));

    let (bundles, errors): (Vec<GtfsBundle>, Vec<ScheduleError>) = archive_paths
        .par_chunks(chunk_size)
        .map(|chunk| {
            chunk
                .iter()
                .map(|dir_entry| {
                    if let Ok(mut bar) = bar.clone().lock() {
                        let _ = bar.update(1);
                    }
                    let path = dir_entry.path();
                    let bundle_file = path.to_str().ok_or_else(|| {
                        ScheduleError::GtfsApp(format!(
                            "unable to convert directory entry into string: {dir_entry:?}"
                        ))
                    })?;
                    // let edge_list_id = *start_edge_list_id + edge_list_offset;
                    process_bundle(bundle_file, conf.clone()).map_err(|e| {
                        ScheduleError::GtfsApp(format!("while processing {bundle_file}, {e}"))
                    })
                })
                .collect_vec()
        })
        .collect_vec_list()
        .into_iter()
        .flat_map(|chunks| chunks.into_iter().flat_map(|chunk| chunk.into_iter()))
        .collect_vec()
        .into_iter()
        .filter(|e| match e {
            Ok(e) if e.is_empty() => false, // remove empty results
            _ => true,
        })
        .partition_result();

    eprintln!(); // end progress bar

    // handle errors, either by terminating early, or, logging them
    if !errors.is_empty() && !ignore_bad_gtfs {
        return Err(batch_processing_error(&errors));
    } else if !errors.is_empty() {
        // log errors
        for error in errors {
            log::error!("{error}");
        }
    }

    // write results to file
    let (_, write_errors): (Vec<_>, Vec<_>) = bundles
        .into_iter()
        .enumerate()
        .collect_vec()
        .par_chunks(chunk_size)
        .map(|chunk| {
            chunk.iter().map(|(index, bundle)| {
                let edge_list_id = conf.starting_edge_list_id + index;
                write_bundle(bundle, conf.clone(), edge_list_id)
            })
        })
        .collect_vec_list()
        .into_iter()
        .flat_map(|chunks| {
            chunks
                .into_iter()
                .flat_map(|chunk| chunk.into_iter().filter(|r| r.is_err()).collect_vec())
        })
        .collect_vec()
        .into_iter()
        .partition_result();

    if !write_errors.is_empty() {
        Err(batch_processing_error(&write_errors))
    } else {
        Ok(())
    }
}

/// read a single GTFS archive and prepare a Compass EdgeList dataset from it.
/// trips with date outside of [start_date, end_date] are removed.
pub fn process_bundle(
    bundle_file: &str,
    c: Arc<ProcessBundlesConfig>,
) -> Result<GtfsBundle, ScheduleError> {
    log::debug!("process_bundle called for {bundle_file}");
    // read the GTFS archive. pre-process by removing Trips that contain stops
    // which do not map to the road network vertices within the matching distance threshold.
    let gtfs = Arc::new(read_gtfs(bundle_file, c.spatial_index.clone())?);

    // if user provided an extent, use it to filter GTFS archives
    if let Some(extent) = c.extent.as_ref() {
        if !archive_intersects_extent(&gtfs, extent)? {
            return Ok(GtfsBundle::empty());
        }
    }

    // Pre-compute lat,lon location of all stops
    // with `get_stop_location` which returns the lat,lon
    // or the parent's lat,lon if available
    let stop_locations: HashMap<String, Option<Point<f64>>> = gtfs
        .stops
        .iter()
        .map(|(stop_id, stop)| (stop_id.clone(), get_stop_location(stop.clone(), &gtfs)))
        .collect();

    // Construct edge lists
    let mut edge_id: EdgeId = EdgeId(0);
    let mut edges: HashMap<(VertexId, VertexId), GtfsEdge> = HashMap::new();
    let mut date_mapping: HashSet<DateMapping> = HashSet::new();
    for target_date in c.date_mapping_policy.iter() {
        for raw_trip in gtfs.trips.values() {
            // sort the stop_time sequence of the trip before proceeding
            let trip = match SortedTrip::new(raw_trip)? {
                Some(t) => t,
                None => continue,
            };

            // apply date mapping
            let picked_date = c
                .date_mapping_policy
                .pick_date(&target_date, &trip, gtfs.clone())?;
            if target_date != picked_date {
                let route = gtfs.get_route(&trip.route_id).map_err(|_| {
                    ScheduleError::MalformedGtfs(format!(
                        "trip {} references route id {} that does not exist",
                        trip.trip_id, trip.route_id
                    ))
                })?;
                let dm = DateMapping {
                    agency_id: route.agency_id.clone(),
                    route_id: trip.route_id.clone(),
                    service_id: trip.service_id.clone(),
                    target_date,
                    picked_date,
                };
                let _ = date_mapping.insert(dm);
            }

            for (src, dst) in trip.stop_times.windows(2).map(|w| (&w[0], &w[1])) {
                process_schedule(
                    &picked_date,
                    src,
                    dst,
                    &trip,
                    &mut edges,
                    &mut edge_id,
                    c.clone(),
                    gtfs.clone(),
                    &stop_locations,
                )?;
            }
        }
    }

    let edges_sorted = edges
        .into_values()
        .sorted_by_cached_key(|e| e.edge.edge_id)
        .collect_vec();

    let metadata = json! [{
        "agencies": json![&gtfs.agencies],
        "feed_info": json![&gtfs.feed_info],
        "read_duration": json![&gtfs.read_duration],
        "calendar": json![&gtfs.calendar],
        "calendar_dates": json![&gtfs.calendar_dates],
    }];

    let result = GtfsBundle {
        edges: edges_sorted,
        metadata,
        date_mapping,
    };

    Ok(result)
}

/// reads a GTFS archive.
pub fn read_gtfs(gtfs_file: &str, spatial_index: Arc<SpatialIndex>) -> Result<Gtfs, ScheduleError> {
    let mut gtfs = Gtfs::new(gtfs_file)?;
    let mut disconnected_stops = HashSet::new();
    for stop in gtfs.stops.values() {
        let remove_route = match get_stop_location(stop.clone(), &gtfs) {
            None => true,
            Some(point) => match_closest_graph_id(&point, spatial_index.clone()).is_err(),
        };
        if remove_route {
            disconnected_stops.insert(&stop.id);
        };
    }
    let mut disconnected_trips = HashSet::new();
    let trip_ids = gtfs.trips.keys().collect_vec();
    for trip_id in trip_ids {
        let trip = gtfs.get_trip(trip_id)?;
        for stop_time in trip.stop_times.iter() {
            if disconnected_stops.contains(&stop_time.stop.id) {
                disconnected_trips.insert(trip.id.clone());
                break;
            }
        }
    }
    for trip_id in disconnected_trips.iter() {
        gtfs.trips.remove(trip_id);
    }
    if !disconnected_stops.is_empty() {
        log::info!(
            "removed {} stops, {} trips due to map matching threshold",
            disconnected_stops.len(),
            disconnected_trips.len(),
        )
    }
    Ok(gtfs)
}

/// writes the provided bundle to files enumerated by the provided edge_list_id.
pub fn write_bundle(
    bundle: &GtfsBundle,
    c: Arc<ProcessBundlesConfig>,
    edge_list_id: usize,
) -> Result<(), ScheduleError> {
    // Write to files
    let output_directory = Path::new(&c.output_directory);

    let metadata_filename = format!("edges-gtfs-metadata-{edge_list_id}.json");
    std::fs::create_dir_all(output_directory).map_err(|e| {
        let outdir = output_directory.to_str().unwrap_or_default();
        ScheduleError::GtfsApp(format!(
            "unable to create output directory path '{outdir}': {e}"
        ))
    })?;

    // update the metadata with fully-qualified route ids
    let mut metadata = bundle.metadata.clone();
    let date_mapping = construct_fq_date_mapping(&bundle.date_mapping, edge_list_id);
    let fq_route_ids = construct_fq_route_id_list(bundle, edge_list_id);
    metadata["date_mapping"] = json![date_mapping];
    metadata["fq_route_ids"] = json![fq_route_ids];

    let metadata_str = serde_json::to_string_pretty(&metadata).map_err(|e| {
        ScheduleError::GtfsApp(format!("failure writing GTFS Agencies as JSON string: {e}"))
    })?;
    std::fs::write(output_directory.join(metadata_filename), &metadata_str)
        .map_err(|e| ScheduleError::GtfsApp(format!("failed writing GTFS Agency metadata: {e}")))?;
    let edges_filename = format!("edges-compass-{edge_list_id}.csv.gz");
    let schedules_filename = format!("edges-schedules-{edge_list_id}.csv.gz");
    let geometries_filename = format!("edges-geometries-enumerated-{edge_list_id}.txt.gz");
    let mut edges_writer = create_writer(
        output_directory,
        &edges_filename,
        true,
        QuoteStyle::Necessary,
        c.overwrite,
    );
    let mut schedules_writer = create_writer(
        output_directory,
        &schedules_filename,
        true,
        QuoteStyle::Necessary,
        c.overwrite,
    );
    let mut geometries_writer = create_writer(
        output_directory,
        &geometries_filename,
        false,
        QuoteStyle::Never,
        c.overwrite,
    );

    for GtfsEdge {
        edge,
        geometry,
        schedules,
    } in bundle.edges.iter()
    {
        if let Some(ref mut writer) = edges_writer {
            writer.serialize(edge).map_err(|e| {
                ScheduleError::GtfsApp(format!(
                    "Failed to write to edges file {}: {}",
                    String::from(&edges_filename),
                    e
                ))
            })?;
        }

        if let Some(ref mut writer) = schedules_writer {
            for schedule in schedules.iter() {
                let fq_schedule = FullyQualifiedScheduleRow::new(schedule, edge_list_id);
                writer.serialize(fq_schedule).map_err(|e| {
                    ScheduleError::GtfsApp(format!(
                        "Failed to write to schedules file {}: {}",
                        String::from(&schedules_filename),
                        e
                    ))
                })?;
            }
        }

        if let Some(ref mut writer) = geometries_writer {
            writer
                .serialize(geometry.to_wkt().to_string())
                .map_err(|e| {
                    ScheduleError::GtfsApp(format!(
                        "Failed to write to geometry file {}: {}",
                        String::from(&edges_filename),
                        e
                    ))
                })?;
        }
    }
    Ok(())
}

/// worker function that constructs a schedule row between some src and dst StopTime
/// on the specified date with the following logic:
///  - checks that the departure + arrival times are within date mapping time range, if provided
///  - map matches the stops to the graph.
///  - creates a [GtfsEdge] if one does not yet exist between these vertices.
///  - handles presence of src + dst times and constructs the datetimes to write to our schedule row
///  - adds this schedule row to our GtfsEdge
#[allow(clippy::too_many_arguments)]
fn process_schedule(
    picked_date: &NaiveDate,
    src: &StopTime,
    dst: &StopTime,
    trip: &SortedTrip,
    edges: &mut HashMap<(VertexId, VertexId), GtfsEdge>,
    edge_id: &mut EdgeId,
    c: Arc<ProcessBundlesConfig>,
    gtfs: Arc<Gtfs>,
    stop_locations: &HashMap<String, Option<Point<f64>>>,
) -> Result<Option<ScheduleRow>, ScheduleError> {
    // ignore times not within our expected time range
    if !c.date_mapping_policy.within_time_range(src, dst) {
        return Ok(None);
    }

    // match this stop time pair to the graph or apply optional fallback policy
    let map_match_result = map_match(src, dst, stop_locations, c.spatial_index.clone())?;
    let ((src_id, src_point), (dst_id, dst_point)) =
        match (map_match_result, &c.missing_stop_location_policy) {
            (Some(result), _) => result,
            (None, MissingStopLocationPolicy::Fail) => {
                let msg = format!("{} or {}", src.stop.id, dst.stop.id);
                return Err(ScheduleError::MissingStopLocationAndParent(msg));
            }
            (None, MissingStopLocationPolicy::DropStop) => return Ok(None),
        };

    // This only gets to run if all previous conditions are met
    // it adds the edge if it has not yet been added.
    let gtfs_edge = edges.entry((src_id, dst_id)).or_insert_with(|| {
        let geometry = match &c.distance_calculation_policy {
            DistanceCalculationPolicy::Haversine => LineString::new(vec![src_point.0, dst_point.0]),
            DistanceCalculationPolicy::Shape => todo!(),
            DistanceCalculationPolicy::Fallback => todo!(),
        };

        // Estimate distance
        let distance: Length = match &c.distance_calculation_policy {
            DistanceCalculationPolicy::Haversine => compute_haversine(src_point, dst_point),
            DistanceCalculationPolicy::Shape => todo!(),
            DistanceCalculationPolicy::Fallback => todo!(),
        };

        let edge = EdgeConfig {
            edge_id: *edge_id,
            src_vertex_id: src_id,
            dst_vertex_id: dst_id,
            distance: distance.get::<uom::si::length::meter>(),
        };

        let gtfs_edge = GtfsEdge::new(edge, geometry);

        // NOTE: edge id update completed after creating this Edge
        *edge_id = EdgeId(edge_id.0 + 1);

        gtfs_edge
    });

    let (src_departure_time, dst_arrival_time) = match (src.departure_time, dst.arrival_time) {
        (None, Some(t)) | (Some(t), None) => {
            let dt = create_datetime(t, picked_date)?;
            Ok((dt, dt))
        }
        (Some(dep), Some(arr)) => {
            let dep_dt = create_datetime(dep, picked_date)?;
            let arr_dt = create_datetime(arr, picked_date)?;
            Ok((dep_dt, arr_dt))
        }
        (None, None) => Err(ScheduleError::MissingAllStopTimes(src.stop.id.clone())),
    }?;

    let route = gtfs.routes.get(&trip.route_id).ok_or_else(|| {
        ScheduleError::MalformedGtfs(format!(
            "trip {} has route id {} which is missing from the archive",
            trip.trip_id, trip.route_id
        ))
    })?;

    // update schedules + date mapping
    let schedule = ScheduleRow::new(
        gtfs_edge.edge.edge_id.0,
        trip.route_id.clone(),
        trip.service_id.clone(),
        route.agency_id.clone(),
        src_departure_time,
        dst_arrival_time,
    );

    gtfs_edge.add_schedule(schedule.clone());

    Ok(Some(schedule))
}

/// helper to build a datetime value from the gtfs time of day and some
/// date (target or picked). compatible with over-midnight time values.
fn create_datetime(gtfs_time: u32, date: &NaiveDate) -> Result<NaiveDateTime, ScheduleError> {
    let offset = Duration::seconds(gtfs_time as i64);
    let datetime = date
        .and_hms_opt(0, 0, 0)
        .and_then(|datetime| {
            datetime.checked_add_signed(offset)
        })
        .ok_or_else(|| {
            let picked_str = date.format("%m-%d-%Y");
            let msg = format!("appending departure offset '{offset}' to picked_date '{picked_str}' produced an empty result (invalid combination)");
            ScheduleError::InvalidData(msg)
        })?;
    Ok(datetime)
}

// Checks the stop and its parent for lon,lat location. Returns None if this fails (parent doesn't exists or doesn't have location)
fn get_stop_location(stop: Arc<Stop>, gtfs: &Gtfs) -> Option<Point<f64>> {
    // Happy path, we have the info in this point
    // lon,lat is required if `location_type` in [0, 1, 2]
    if let (Some(lon), Some(lat)) = (stop.longitude, stop.latitude) {
        return Some(Point::new(lon, lat));
    }

    // Use lon,lat from parent station if data is missing. `parent_station` is required for `location_type=3 or 4`
    //
    // This could be done recursively but I think fixing it to
    // look only one step further is better. If this doesn't work
    // there are some wrong assumptions about the data
    stop.parent_station
        .clone()
        .and_then(|parent_id| gtfs.stops.get(&parent_id))
        .and_then(
            |parent_stop| match (parent_stop.longitude, parent_stop.latitude) {
                (Some(lon), Some(lat)) => Some(Point::new(lon, lat)),
                _ => None,
            },
        )
}

/// helper function that creates a list of all unique, fully-qualified route ids in this
/// bundle, sorted lexicagraphically.
fn construct_fq_route_id_list(bundle: &GtfsBundle, edge_list_id: usize) -> Vec<String> {
    bundle
        .edges
        .iter()
        .flat_map(|e| {
            e.schedules.iter().map(|s| {
                fq_ops::get_fully_qualified_route_id(
                    s.agency_id.as_deref(),
                    &s.route_id,
                    &s.service_id,
                    edge_list_id,
                )
            })
        })
        .collect::<HashSet<_>>() // dedup unordered
        .into_iter()
        .sorted()
        .collect_vec()
}

/// helper function to build the nested map for date mapping using the fully-qualified route ids
fn construct_fq_date_mapping(
    dms: &HashSet<DateMapping>,
    edge_list_id: usize,
) -> HashMap<String, HashMap<NaiveDate, NaiveDate>> {
    dms.iter()
        .map(|dm| {
            (
                dm.get_fully_qualified_id(edge_list_id),
                (dm.target_date, dm.picked_date),
            )
        })
        .into_group_map()
        .into_iter()
        .map(|(k, pairs)| (k, pairs.into_iter().collect()))
        .collect()
}

pub type MapMatchResult = ((VertexId, Point<f64>), (VertexId, Point<f64>));
/// finds the vertex and point associated with src and dst StopTime entry.
///
/// # Result
///
/// the source and destination, each a tuple of (VertexId, Coordinate)
fn map_match(
    src: &StopTime,
    dst: &StopTime,
    stop_locations: &HashMap<String, Option<Point<f64>>>,
    spatial_index: Arc<SpatialIndex>,
) -> Result<Option<MapMatchResult>, ScheduleError> {
    // Since `stop_locations` is computed from `gtfs.stops`, this should never fail
    let maybe_src = stop_locations.get(&src.stop.id).ok_or_else(|| {
        ScheduleError::MalformedGtfs(format!(
            "source stop_id '{}' is not associated with a geographic location in either it's stop row or any parent row (see 'parent_station' of GTFS Stops.txt)",
            src.stop.id
        ))
    })?;
    let maybe_dst = stop_locations.get(&dst.stop.id).ok_or_else(|| {
        ScheduleError::MalformedGtfs(format!(
            "destination stop_id '{}' is not associated with a geographic location in either it's stop row or any parent row (see 'parent_station' of GTFS Stops.txt)",
            dst.stop.id
        ))
    })?;

    match (maybe_src, maybe_dst) {
        (Some(src_point_), Some(dst_point_)) => {
            // If you can find both:
            // Map to closest compass vertex
            let src_compass = match_closest_graph_id(src_point_, spatial_index.clone())?;
            let dst_compass = match_closest_graph_id(dst_point_, spatial_index.clone())?;

            // These points are used to compute the distance
            // Should we instead be using the graph node?
            // For instance, what happens if src_compass == dst_compass?
            let src_point = src_point_.to_owned();
            let dst_point = dst_point_.to_owned();
            Ok(Some(((src_compass, src_point), (dst_compass, dst_point))))
        }
        _ => Ok(None),
    }
}

/// helper function for map matching stop locations to the graph.
fn match_closest_graph_id(
    point: &Point<f64>,
    spatial_index: Arc<SpatialIndex>,
) -> Result<VertexId, ScheduleError> {
    let point_f32 = Point::new(point.x() as f32, point.y() as f32);

    // This fails if: 1) The spatial index fails, or 2) it returns an edge
    let nearest_result = spatial_index.nearest_graph_id(&point_f32)?;
    match nearest_result {
        NearestSearchResult::NearestVertex(vertex_id) => Ok(vertex_id),
        _ => Err(ScheduleError::GtfsApp(format!(
            "could not find matching vertex for point {} in spatial index. consider expanding the distance tolerance or allowing for stop filtering.",
            point.to_wkt()
        ))),
    }
}

/// helper function to build a filewriter for writing either .csv.gz or
/// .txt.gz files for compass datasets while respecting the user's overwrite
/// preferences and properly formatting WKT outputs.
fn create_writer(
    directory: &Path,
    filename: &str,
    has_headers: bool,
    quote_style: QuoteStyle,
    overwrite: bool,
) -> Option<csv::Writer<GzEncoder<File>>> {
    let filepath = directory.join(filename);
    if filepath.exists() && !overwrite {
        return None;
    }
    let file = File::create(filepath).unwrap();
    let buffer = GzEncoder::new(file, Compression::default());
    let writer = csv::WriterBuilder::new()
        .has_headers(has_headers)
        .quote_style(quote_style)
        .from_writer(buffer);
    Some(writer)
}

/// tests if a GTFS archive has at least one Stop that intersects the provided extent.
fn archive_intersects_extent(gtfs: &Gtfs, extent: &Geometry) -> Result<bool, ScheduleError> {
    for stop in gtfs.stops.values() {
        if let Some(point) = get_stop_location(stop.clone(), gtfs) {
            if extent.contains(&point) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
