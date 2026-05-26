// use crate::agency::read_agency_from_flex;
// use crate::calendar::{read_calendar_from_flex, Calendar};
// use crate::routes::{read_routes_from_flex, Route};
// use crate::stop_times::{read_stop_times_from_flex, StopTimes};
// use crate::trips::{read_trips_from_flex, Trips};

use crate::model::GtfsFlexError;
use crate::util::zone::ZoneGeometry;
use crate::util::zone::ZoneId;
use crate::util::zone::ZoneRecord;
use chrono::Datelike;
use chrono::NaiveTime;
use geozero::ToWkt;
use gtfs_structures::Location;
use gtfs_structures::{Calendar, Gtfs, PickupDropOffType, StopTime, Trip};
use kdam::tqdm;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

const ZONE_IDS_FILENAME: &str = "zone_ids_enumerated.txt.gz";
const RECORDS_FILENAME: &str = "records.csv.gz";
const GEOMETRIES_FILENAME: &str = "geometries.csv.gz";

#[derive(Default, Debug)]
pub struct GtfsFlexDataset {
    records: Vec<ZoneRecord>,
    geometries: Vec<ZoneGeometry>,
    zone_ids: Vec<ZoneId>,
}

impl GtfsFlexDataset {
    /// mutably extends this collection by consuming the rhs collection.
    pub fn extend(&mut self, rhs: GtfsFlexDataset) {
        self.records.extend(rhs.records);
        self.geometries.extend(rhs.geometries);
        self.zone_ids.extend(rhs.zone_ids);
    }

    pub fn write(&self, dir: &Path) -> Result<(), GtfsFlexError> {
        let zones_writer = crate::util::fs::create_writer(
            dir,
            ZONE_IDS_FILENAME,
            false,
            csv::QuoteStyle::Necessary,
            false,
        );
        let geoms_writer = crate::util::fs::create_writer(
            dir,
            GEOMETRIES_FILENAME,
            true,
            csv::QuoteStyle::Necessary,
            false,
        );
        let records_writer = crate::util::fs::create_writer(
            dir,
            RECORDS_FILENAME,
            true,
            csv::QuoteStyle::Necessary,
            false,
        );

        if let Some(mut w) = zones_writer {
            let iter = tqdm!(
                self.zone_ids.iter(),
                desc = format!("writing {ZONE_IDS_FILENAME}"),
                total = self.zone_ids.len()
            );
            for zone_id in iter {
                w.serialize(zone_id)
                    .map_err(|error| GtfsFlexError::CsvWrite {
                        path: dir.join(ZONE_IDS_FILENAME),
                        error,
                    })?;
            }
        }
        if let Some(mut w) = records_writer {
            let iter = tqdm!(
                self.records.iter(),
                desc = format!("writing {RECORDS_FILENAME}"),
                total = self.records.len()
            );
            for record in iter {
                w.serialize(record)
                    .map_err(|error| GtfsFlexError::CsvWrite {
                        path: dir.join(RECORDS_FILENAME),
                        error,
                    })?;
            }
        }
        if let Some(mut w) = geoms_writer {
            let iter = tqdm!(
                self.geometries.iter(),
                desc = format!("writing {GEOMETRIES_FILENAME}"),
                total = self.geometries.len()
            );
            for geometry in iter {
                w.serialize(geometry)
                    .map_err(|error| GtfsFlexError::CsvWrite {
                        path: dir.join(GEOMETRIES_FILENAME),
                        error,
                    })?;
            }
        }

        Ok(())
    }
}

/// process all GTFS-Flex feeds in the given directory
pub fn process_gtfs_flex_bundle(
    flex_directory_path: &Path,
    out_directory_path: &Path,
    date_requested: &str,
) -> Result<(), GtfsFlexError> {
    println!("=== Processing GTFS-Flex bundle ===");

    // discover gtfs-flex feeds
    discover_gtfs_flex_feeds(flex_directory_path)?;

    // process files in each feed
    let gtfs_flex_dataset = process_flex_files(flex_directory_path, date_requested)?;

    gtfs_flex_dataset.write(out_directory_path)?;

    println!("=== GTFS-Flex processing complete ===");
    Ok(())
}

/// discover all zip files in the given directory
pub fn discover_gtfs_flex_feeds(flex_directory_path: &Path) -> Result<(), GtfsFlexError> {
    if !flex_directory_path.exists() {
        log::error!("Directory does not exist: {:?}", flex_directory_path);
        return Ok(());
    }

    let entries = fs::read_dir(flex_directory_path).map_err(|error| GtfsFlexError::Io {
        path: flex_directory_path.to_path_buf(),
        error,
    })?;

    println!("Found zip files in {:?}:", flex_directory_path);

    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|error| GtfsFlexError::Io {
            path: flex_directory_path.to_path_buf(),
            error,
        })?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "zip") {
            if let Some(name) = path.file_name() {
                println!("      {}", name.to_string_lossy());
                count += 1;
            }
        }
    }

    println!("Total GTFS-flex feeds found: {}", count);

    Ok(())
}

/// iterate over gtfs-flex feeds and process files from each feed
/// return valid zones for the requested date
pub fn process_flex_files(
    flex_directory_path: &Path,
    date_requested: &str,
) -> Result<GtfsFlexDataset, GtfsFlexError> {
    println!("Processing GTFS-Flex feeds in {:?}", flex_directory_path);

    let iter = std::fs::read_dir(flex_directory_path).map_err(|error| GtfsFlexError::Io {
        path: flex_directory_path.to_path_buf(),
        error,
    })?;

    // read each archive (.zip file) found in the directory
    let mut dataset: GtfsFlexDataset = Default::default();
    for (idx, entry) in iter.enumerate() {
        let entry = entry.map_err(|error| GtfsFlexError::Io {
            path: flex_directory_path.to_path_buf(),
            error,
        })?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "zip") {
            println!("  Processing {:?}", path);

            // extract feed name
            let feed_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown_feed")
                .to_string();

            // load GTFS
            let gtfs = Gtfs::from_path(&path).map_err(|error| GtfsFlexError::GtfsRead {
                path: path.to_path_buf(),
                error,
            })?;

            // process files for requested date and time and get valid zones
            let archive_dataset = process_archive(&gtfs, date_requested, &feed_name, idx)?;
            dataset.extend(archive_dataset);
        }
    }

    println!("GTFS-Flex feeds processed!");

    Ok(dataset)
}

/// process calender, trips, routes, and stop_times files for the requested date and time
pub fn process_archive(
    gtfs: &Gtfs,
    date_requested: &str,
    feed_name: &str,
    archive_idx: usize,
) -> Result<GtfsFlexDataset, GtfsFlexError> {
    // parse requested date
    let date = chrono::NaiveDate::parse_from_str(date_requested, "%Y%m%d").map_err(|e| {
        let msg = format!("user date request is invalid: {e}");
        GtfsFlexError::Runtime(msg)
    })?;

    println!("          requested date: {:?}", date);

    // filter calendar for the requested date
    let weekday = match date.weekday() {
        chrono::Weekday::Mon => |c: &Calendar| c.monday,
        chrono::Weekday::Tue => |c: &Calendar| c.tuesday,
        chrono::Weekday::Wed => |c: &Calendar| c.wednesday,
        chrono::Weekday::Thu => |c: &Calendar| c.thursday,
        chrono::Weekday::Fri => |c: &Calendar| c.friday,
        chrono::Weekday::Sat => |c: &Calendar| c.saturday,
        chrono::Weekday::Sun => |c: &Calendar| c.sunday,
    };
    println!("          requested day: {:?}", date.weekday());

    let active_service_ids: Vec<&str> = gtfs
        .calendar
        .values()
        .filter(|c| weekday(c) && c.start_date <= date && date <= c.end_date)
        .map(|c| c.id.as_str())
        .collect();

    // println!("          active service_ids: {:?}", active_service_ids);

    // 1. Map route_id -> agency_id (fallback to archive_idx if missing)
    // at the end, route_to_agency contains ALL AgencyIds referenced by
    let default_agency_id = format!("archive{archive_idx}");
    let mut route_to_agency: HashMap<&str, String> = HashMap::new();
    for route in gtfs.routes.values() {
        if !route_to_agency.contains_key(route.id.as_str()) {
            let agency_id = match &route.agency_id {
                Some(id) => id.clone(),
                None => default_agency_id.clone(),
            };
            route_to_agency.insert(&route.id, agency_id);
        }
    }

    // filter trips by active service_ids and map trip_id -> agency_id
    let mut trip_to_agency: HashMap<&str, String> = HashMap::new();
    let active_trips: Vec<&Trip> = gtfs
        .trips
        .values()
        .filter(|t| active_service_ids.contains(&t.service_id.as_str()))
        .inspect(|t| {
            if let Some(agency_id) = route_to_agency.get(t.route_id.as_str()) {
                trip_to_agency.insert(&t.id, agency_id.to_string());
            }
        })
        .collect();

    // create each output dataset using a fully-qualified ZoneId identifier for each record.
    let mut records = vec![];
    let mut geometries = vec![];
    let mut all_zone_ids = HashSet::new();
    for trip in active_trips.into_iter() {
        match trip.stop_times.as_slice() {
            [src, dst] if valid_flex_trip_stops(src, dst) => {
                let resolved_agency_id = trip_to_agency.get(&trip.id.as_str())
                    .cloned()
                    .ok_or_else(|| {
                        let msg = format!("after ensuring bijection from routes to agencies, found trip {} had no agency", trip.id);
                        GtfsFlexError::Internal(msg)
                    })?;
                let route_id = &trip.route_id;
                let trip_id = &trip.id;
                let (src_loc, dst_loc) = get_locations(src, dst, trip)?;
                let src_loc_id = &src_loc.id;
                let dst_loc_id = &dst_loc.id;
                let src_zone_id =
                    ZoneId::from_full_namespace(&resolved_agency_id, route_id, trip_id, src_loc_id);
                let dst_zone_id =
                    ZoneId::from_full_namespace(&resolved_agency_id, route_id, trip_id, dst_loc_id);
                let start_pickup_drop_off_window = src
                    .start_pickup_drop_off_window
                    .and_then(|s| NaiveTime::from_num_seconds_from_midnight_opt(s, 0));
                let end_pickup_drop_off_window = dst
                    .end_pickup_drop_off_window
                    .and_then(|s| NaiveTime::from_num_seconds_from_midnight_opt(s, 0));
                let src_geometry = match &src_loc.geometry {
                    gtfs_structures::LocationGeometry::Polygon(geometry) => geometry,
                    gtfs_structures::LocationGeometry::MultiPolygon(geometry) => geometry,
                };
                let geom: geo_types::Geometry<f64> = src_geometry.try_into().map_err(|e| {
                    let msg = format!("failed to parse GeoJSON geometry into Geo geometry: {e}");
                    GtfsFlexError::Runtime(msg)
                })?;

                let wkt_str = geom
                    .to_wkt()
                    .map_err(|e| GtfsFlexError::Runtime(format!("WKT error: {e}")))?;

                records.push(ZoneRecord {
                    agency_id: resolved_agency_id,
                    feed: feed_name.to_string(),
                    requested_date: date_requested.to_string(),
                    trip_id: trip.id.clone(),
                    origin_zone: src_zone_id.clone(),
                    start_pickup_drop_off_window,
                    end_pickup_drop_off_window,
                    destination_zone: dst_zone_id.clone(),
                });

                all_zone_ids.insert(src_zone_id.clone());
                all_zone_ids.insert(dst_zone_id.clone());

                geometries.push(ZoneGeometry {
                    zone_id: src_zone_id.clone(),
                    geometry: wkt_str,
                });
            }
            other => {
                log::warn!(
                    "GTFS-Flex Trip {} has {} StopTime entries, assumed should always be 2",
                    trip.id,
                    other.len()
                );
            }
        }
    }

    let mut zone_ids = all_zone_ids.into_iter().collect::<Vec<_>>();
    zone_ids.sort();
    Ok(GtfsFlexDataset {
        records,
        geometries,
        zone_ids,
    })
}

fn valid_flex_trip_stops(src: &StopTime, dst: &StopTime) -> bool {
    matches!(src.pickup_type, PickupDropOffType::ArrangeByPhone)
        && matches!(src.drop_off_type, PickupDropOffType::NotAvailable)
        && matches!(dst.pickup_type, PickupDropOffType::NotAvailable)
        && matches!(dst.drop_off_type, PickupDropOffType::ArrangeByPhone)
}

/// helper to grab the locations of two [StopTime]s along some [Trip].
fn get_locations(
    src: &StopTime,
    dst: &StopTime,
    trip: &Trip,
) -> Result<(Arc<Location>, Arc<Location>), GtfsFlexError> {
    let locations = (src.location.clone(), dst.location.clone());
    match locations {
        (Some(sloc), Some(dloc)) => Ok((sloc, dloc)),
        (None, Some(_)) => Err(GtfsFlexError::Runtime(format!(
            "GTFS-Flex Trip {} has origin StopTime that is missing a 'location' entry",
            trip.id
        ))),
        (Some(_), None) => Err(GtfsFlexError::Runtime(format!(
            "GTFS-Flex Trip {} has destination StopTime that is missing a 'location' entry",
            trip.id
        ))),
        (None, None) => Err(GtfsFlexError::Runtime(format!(
            "GTFS-Flex Trip {} has two StopTimes, both missing a 'location' entry",
            trip.id
        ))),
    }
}
