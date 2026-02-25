use crate::calendar::{read_calendar_from_flex, Calendar};
use crate::locations::{read_locations_from_flex, Location};
use crate::stop_times::{read_stop_times_from_flex, StopTimes};
use crate::trips::{read_trips_from_flex, Trips};

use chrono::Datelike;
use chrono::NaiveTime;
use geo_types::Geometry;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// a valid origin-destination zone pair for a trip
#[derive(Debug)]
pub struct ValidZone {
    trip_id: String,
    origin_zone: String,
    destination_zone: String,
    start_pickup_drop_off_window: Option<NaiveTime>,
    end_pickup_drop_off_window: Option<NaiveTime>,
    origin_zone_geom: Option<Geometry>,
    destination_zone_geom: Option<Geometry>,
}

/// process all GTFS-Flex feeds in the given directory
pub fn process_gtfs_flex_bundle(
    flex_directory_path: &Path,
    date_requested: &str,
) -> io::Result<()> {
    println!("=== Processing GTFS-Flex bundle ===");

    // discover gtfs-flex feeds
    discover_gtfs_flex_feeds(flex_directory_path)?;

    // process files in each feed
    process_flex_files(flex_directory_path, date_requested)?;

    println!("=== GTFS-Flex processing complete ===");
    Ok(())
}

/// discover all zip files in the given directory
pub fn discover_gtfs_flex_feeds(flex_directory_path: &Path) -> io::Result<()> {
    if !flex_directory_path.exists() {
        eprintln!("Directory does not exist: {:?}", flex_directory_path);
        return Ok(());
    }

    let entries = fs::read_dir(flex_directory_path)?;

    println!("Found zip files in {:?}:", flex_directory_path);

    let mut count = 0;
    for entry in entries {
        let entry = entry?;
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
pub fn process_flex_files(flex_directory_path: &Path, date_requested: &str) -> io::Result<()> {
    println!("Processing GTFS-Flex feeds in {:?}", flex_directory_path);

    for entry in std::fs::read_dir(flex_directory_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "zip") {
            println!("  Processing {:?}", path);

            // read calender.txt
            let calendar = read_calendar_from_flex(&path)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Error in calendar.txt")
            })?;
            // println!("      calendar.txt records: {:?}", calendar);
            println!("      calendar.txt read!");

            // read trips.txt
            let trips = read_trips_from_flex(&path)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Error in trips.txt"))?;
            // println!("      trips.txt records: {:?}", trips);
            println!("      trips.txt read!");

            // read stop_times.txt
            let stop_times = read_stop_times_from_flex(&path)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Error in stop_times.txt")
            })?;
            // println!("      stop_times.txt records: {:?}", stop_times);
            println!("      stop_times.txt read!");

            // read locations.geojson
            let locations = read_locations_from_flex(&path)?.ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Error in locations.geojson")
            })?;
            // println!("      locations.geojson content: {:?}", locations);
            println!("      locations.geojson read!");

            // process files for requested date and time and get valid zones
            let valid_zones =
                join_flex_files(&calendar, &trips, &stop_times, &locations, date_requested)?;

            println!(
                "Valid zones (trip_id, origin_zone, destination_zone, start_pickup_drop_off_window, end_pickup_drop_off_window, origin_zone_geom, destination_zone_geom): {:#?}",
                valid_zones
                    .iter()
                    .map(|vz| {
                        (
                            &vz.trip_id,
                            &vz.origin_zone,
                            &vz.destination_zone,
                            vz.start_pickup_drop_off_window,
                            vz.end_pickup_drop_off_window,
                            vz.origin_zone_geom.as_ref().map(|g| format!("{:?}", g).chars().take(50).collect::<String>() + "..."),
                            vz.destination_zone_geom.as_ref().map(|g| format!("{:?}", g).chars().take(50).collect::<String>() + "..."),
                        )
                    })
                    .collect::<Vec<_>>()
            );
        }
    }

    println!("GTFS-Flex feeds processed!");

    Ok(())
}

/// process calender, trips, stop_times, and locaitons files for the requested date and time
pub fn join_flex_files(
    calendar: &[Calendar],
    trips: &[Trips],
    stop_times: &[StopTimes],
    _locations: &[Location],
    date_requested: &str,
) -> io::Result<Vec<ValidZone>> {
    // parse requested date
    let date = chrono::NaiveDate::parse_from_str(date_requested, "%Y%m%d")
        .expect("Invalid date format YYYYMMDD");

    println!("          requested date: {:?}", date);

    // filter calendar for the requested date
    let weekday = match date.weekday() {
        chrono::Weekday::Mon => |c: &Calendar| c.monday == 1,
        chrono::Weekday::Tue => |c: &Calendar| c.tuesday == 1,
        chrono::Weekday::Wed => |c: &Calendar| c.wednesday == 1,
        chrono::Weekday::Thu => |c: &Calendar| c.thursday == 1,
        chrono::Weekday::Fri => |c: &Calendar| c.friday == 1,
        chrono::Weekday::Sat => |c: &Calendar| c.saturday == 1,
        chrono::Weekday::Sun => |c: &Calendar| c.sunday == 1,
    };
    println!("          requested day: {:?}", date.weekday());

    let active_service_ids: Vec<&str> = calendar
        .iter()
        .filter(|c| weekday(c) && c.start_date <= date && date <= c.end_date)
        .map(|c| c.service_id.as_str())
        .collect();

    println!("          active service_ids: {:?}", active_service_ids);

    // filter trips by active service_ids
    let active_trips: Vec<&Trips> = trips
        .iter()
        .filter(|t| active_service_ids.contains(&t.service_id.as_str()))
        .collect();

    // println!("          active trips: {:?}", active_trips);

    // filter stop_times for active trips and by requested time
    let active_trip_ids: Vec<&str> = active_trips.iter().map(|t| t.trip_id.as_str()).collect();
    let active_stop_times: Vec<&StopTimes> = stop_times
        .iter()
        .filter(|st| active_trip_ids.contains(&st.trip_id.as_str()))
        .collect();
    // println!("          active stop_times: {:?}", active_stop_times);

    // build location lookup map
    let location_map: HashMap<String, &Location> =
        locations.iter().map(|loc| (loc.id.clone(), loc)).collect();

    // group stop_times by trip_id
    let mut stop_times_by_trip: HashMap<String, Vec<&StopTimes>> = HashMap::new();
    for st in &active_stop_times {
        stop_times_by_trip
            .entry(st.trip_id.clone())
            .or_default()
            .push(*st);
    }

    // create valid zones of origin-destination pairs from each trip in stop_times
    let valid_zones: Vec<ValidZone> = stop_times_by_trip
        .into_iter()
        .filter_map(|(trip_id, sts)| {
            // find origin zone: pickup allowed, dropoff not allowed
            let origin = sts
                .iter()
                .find(|st| st.pickup_type == 2 && st.drop_off_type == 1)
                .map(|st| st.location_id.clone());

            // find destination zone: pickup not allowed, dropoff allowed
            let destination = sts
                .iter()
                .find(|st| st.pickup_type == 1 && st.drop_off_type == 2)
                .map(|st| st.location_id.clone());

            // find start pickup/drop-off window
            // using value from origin zone row
            let start_pickup_drop_off_window = sts
                .iter()
                .find(|st| st.pickup_type == 2 && st.drop_off_type == 1)
                .map(|st| st.start_pickup_drop_off_window);

            // find end pickup/drop-off window
            // using value from destination zone row
            let end_pickup_drop_off_window = sts
                .iter()
                .find(|st| st.pickup_type == 1 && st.drop_off_type == 2)
                .map(|st| st.end_pickup_drop_off_window);

            // append geometries to origin and destination zones
            match (origin, destination) {
                (Some(origin_zone), Some(destination_zone)) => {
                    let origin_zone_geom = location_map
                        .get(&origin_zone)
                        .map(|loc| loc.geometry.clone());

                    let destination_zone_geom = location_map
                        .get(&destination_zone)
                        .map(|loc| loc.geometry.clone());

                    Some(ValidZone {
                        trip_id,
                        origin_zone,
                        start_pickup_drop_off_window,
                        end_pickup_drop_off_window,
                        destination_zone,
                        origin_zone_geom,
                        destination_zone_geom,
                    })
                }
                _ => None,
            }
        })
        .collect();

    Ok(valid_zones)
}
