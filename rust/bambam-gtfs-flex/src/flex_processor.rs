// use crate::agency::read_agency_from_flex;
// use crate::calendar::{read_calendar_from_flex, Calendar};
// use crate::routes::{read_routes_from_flex, Route};
// use crate::stop_times::{read_stop_times_from_flex, StopTimes};
// use crate::trips::{read_trips_from_flex, Trips};

use bambam_gtfs_flex::model::GtfsFlexError;
use chrono::Datelike;
use chrono::NaiveTime;
use gtfs_structures::{Calendar, Gtfs, PickupDropOffType, StopTime, Trip};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// a valid origin-destination zone pair for a trip
#[derive(Debug, Serialize)]
pub struct ValidZone {
    pub agency_id: Option<String>,
    pub feed: String,
    pub requested_date: String,
    pub trip_id: String,
    pub origin_zone: String,
    pub start_pickup_drop_off_window: Option<NaiveTime>,
    pub end_pickup_drop_off_window: Option<NaiveTime>,
    pub destination_zone: String,
}

/// process all GTFS-Flex feeds in the given directory
pub fn process_gtfs_flex_bundle(
    flex_directory_path: &Path,
    date_requested: &str,
) -> Result<Vec<ValidZone>, GtfsFlexError> {
    println!("=== Processing GTFS-Flex bundle ===");

    // discover gtfs-flex feeds
    discover_gtfs_flex_feeds(flex_directory_path)?;

    // process files in each feed
    let all_valid_zones = process_flex_files(flex_directory_path, date_requested)?;

    println!("=== GTFS-Flex processing complete ===");
    Ok(all_valid_zones)
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
) -> Result<Vec<ValidZone>, GtfsFlexError> {
    println!("Processing GTFS-Flex feeds in {:?}", flex_directory_path);

    let mut all_valid_zones = Vec::new();
    let iter = std::fs::read_dir(flex_directory_path).map_err(|error| GtfsFlexError::Io {
        path: flex_directory_path.to_path_buf(),
        error,
    })?;

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

            // // read agency.txt
            // // read checking for error but ignoring actual multi-agency constraints
            // let _agencies = read_agency_from_flex(&path)?
            //     .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "agency.txt missing"))?;
            // println!("      agency.txt read!");

            // // read calender.txt
            // let calendar = read_calendar_from_flex(&path)?.ok_or_else(|| {
            //     io::Error::new(io::ErrorKind::InvalidData, "Error in calendar.txt")
            // })?;
            // // println!("      calendar.txt records: {:?}", calendar);
            // println!("      calendar.txt read!");

            // // read trips.txt
            // let trips = read_trips_from_flex(&path)?
            //     .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Error in trips.txt"))?;
            // // println!("      trips.txt records: {:?}", trips);
            // println!("      trips.txt read!");

            // // read stop_times.txt
            // let stop_times = read_stop_times_from_flex(&path)?.ok_or_else(|| {
            //     io::Error::new(io::ErrorKind::InvalidData, "Error in stop_times.txt")
            // })?;
            // // println!("      stop_times.txt records: {:?}", stop_times);
            // println!("      stop_times.txt read!");

            // // read routes.txt
            // let routes = read_routes_from_flex(&path)?
            //     .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Error in routes.txt"))?;
            // println!("      routes.txt read!");

            // process files for requested date and time and get valid zones
            let valid_zones = join_flex_files(&gtfs, date_requested, &feed_name, idx)?;

            // println!(
            //     "Valid zones (trip_id, origin_zone, destination_zone, start_pickup_drop_off_window, end_pickup_drop_off_window): {:#?}",
            //     valid_zones
            // );

            // valid zones with feed name
            let valid_zones_with_feed: Vec<ValidZone> = valid_zones
                .into_iter()
                .map(|mut zone| {
                    zone.feed = feed_name.clone();
                    zone
                })
                .collect();

            all_valid_zones.extend(valid_zones_with_feed);
        }
    }

    println!("GTFS-Flex feeds processed!");

    Ok(all_valid_zones)
}

/// process calender, trips, routes, and stop_times files for the requested date and time
pub fn join_flex_files(
    gtfs: &Gtfs,
    date_requested: &str,
    feed_name: &str,
    archive_idx: usize,
) -> Result<Vec<ValidZone>, GtfsFlexError> {
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
    let mut route_to_agency: HashMap<&str, String> = HashMap::new();
    for route in gtfs.routes.values() {
        let agency_id = route
            .agency_id
            .clone()
            .unwrap_or_else(|| format!("archive{archive_idx}"));
        route_to_agency.insert(&route.id, agency_id);
    }

    // filter trips by active service_ids and map trip_id -> agency_id
    let mut trip_to_agency: HashMap<&str, String> = HashMap::new();
    let active_trips: Vec<&Trip> = gtfs
        .trips
        .values()
        .filter(|t| active_service_ids.contains(&t.service_id.as_str()))
        .map(|t| {
            if let Some(agency_id) = route_to_agency.get(t.route_id.as_str()) {
                trip_to_agency.insert(&t.id, agency_id.to_string());
            }
            t
        })
        .collect();

    // let stop_times_by_trip: HashMap<String, Vec<StopTime>> = active_trips
    //     .iter()
    //     .map(|t| (t.id.clone(), t.stop_times))
    //     .collect();

    // println!("          active trips: {:?}", active_trips);

    // filter stop_times for active trips and by requested time
    // let active_trip_ids: Vec<&str> = active_trips.iter().map(|t| t.id.as_str()).collect();
    // let active_stop_times: Vec<&StopTime> = gtfs
    //     .trips
    //     .iter()
    //     .stop_times
    //     .values()
    //     .filter(|st| active_trip_ids.contains(&st.trip_id.as_str()))
    //     .collect();
    // println!("          active stop_times: {:?}", active_stop_times);

    // group stop_times by trip_id
    // let mut stop_times_by_trip: HashMap<String, Vec<&StopTimes>> = HashMap::new();
    // for st in &active_stop_times {
    //     stop_times_by_trip
    //         .entry(st.trip_id.clone())
    //         .or_default()
    //         .push(*st);
    // }

    // create valid zones of origin-destination pairs from each trip in stop_times
    let mut valid_zones: Vec<ValidZone> = vec![];
    for trip in active_trips.into_iter() {
        match trip.stop_times.as_slice() {
            [src, dst] if valid_flex_trip_stops(src, dst) => {
                let resolved_agency_id = trip_to_agency.get(&trip.id.as_str()).cloned();
                let zones_opt = (src.location.clone(), dst.location.clone());
                let (origin_zone, destination_zone) = match zones_opt {
                    (Some(sloc), Some(dloc)) => (sloc.id.clone(), dloc.id.clone()),
                    (None, Some(_)) => {
                        log::warn!("GTFS-Flex Trip {} has origin StopTime that is missing a 'location' entry", trip.id);
                        continue;
                    }
                    (Some(_), None) => {
                        log::warn!("GTFS-Flex Trip {} has destination StopTime that is missing a 'location' entry", trip.id);
                        continue;
                    }
                    (None, None) => {
                        log::warn!(
                            "GTFS-Flex Trip {} has two StopTimes, both missing a 'location' entry",
                            trip.id
                        );
                        continue;
                    }
                };
                let start_pickup_drop_off_window = src
                    .start_pickup_drop_off_window
                    .and_then(|s| NaiveTime::from_num_seconds_from_midnight_opt(s, 0));
                let end_pickup_drop_off_window = dst
                    .end_pickup_drop_off_window
                    .and_then(|s| NaiveTime::from_num_seconds_from_midnight_opt(s, 0));
                let valid_zone = ValidZone {
                    agency_id: resolved_agency_id,
                    feed: feed_name.to_string(),
                    requested_date: date_requested.to_string(),
                    trip_id: trip.id.clone(),
                    origin_zone,
                    start_pickup_drop_off_window,
                    end_pickup_drop_off_window,
                    destination_zone,
                };
                valid_zones.push(valid_zone)
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

    Ok(valid_zones)
}

fn valid_flex_trip_stops(src: &StopTime, dst: &StopTime) -> bool {
    matches!(src.pickup_type, PickupDropOffType::ArrangeByPhone)
        && matches!(src.drop_off_type, PickupDropOffType::NotAvailable)
        && matches!(dst.pickup_type, PickupDropOffType::NotAvailable)
        && matches!(dst.drop_off_type, PickupDropOffType::ArrangeByPhone)
}
