//! End-to-end Phase 1 -> Phase 2 -> Phase 3 pipeline integration tests across GTFS and GTFS-Flex fixtures.

use bambam_gtfs::ids::*;
use bambam_gtfs::model::*;
#[cfg(feature = "gtfs-flex")]
use bambam_gtfs::primitives::GtfsTime;
use bambam_gtfs::raw::*;
use bambam_gtfs::routing::*;
#[cfg(feature = "gtfs-flex")]
use bambam_gtfs::traits::Identifiable;
#[cfg(feature = "gtfs-flex")]
use bambam_gtfs::traits::{Spatial, TemporalStopTime};
use chrono::NaiveDate;
use std::convert::TryFrom;
use std::path::PathBuf;

fn fixture_path(rel_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path)
}

#[test]
fn test_phased_pipeline_basic_fixture() {
    // Phase 1: RawFeed
    let mut raw =
        RawFeed::from_path(fixture_path("fixtures/basic")).expect("Phase 1 RawFeed should succeed");
    assert_eq!(raw.agencies.len(), 2);
    assert_eq!(raw.stops.len(), 6);
    assert_eq!(raw.routes.len(), 3);
    assert_eq!(raw.trips.len(), 1);
    assert_eq!(raw.stop_times.len(), 3);

    // Attach A_shp to trip1 so we also verify automatic stop-coordinate LRS projection onto ShapePolyline
    raw.trips[0].shape_id = Some(ShapeId::new("A_shp"));

    // Phase 2: ValidatedFeed
    let validated = ValidatedFeed::try_from(raw).expect("Phase 2 ValidatedFeed should succeed");
    assert_eq!(validated.stops.len(), 6);
    assert_eq!(validated.routes.len(), 4); // 3 in routes.txt + 1 implicit from trips.txt ("route1")
    assert_eq!(validated.trips.len(), 1);

    // Verify calendar + calendar_dates unified in ServiceCalendar
    let service1 = validated
        .services
        .get(&ServiceId::new("service1"))
        .expect("service1 should exist");
    // 2017-01-07 is a Saturday (active in weekly pattern)
    assert!(service1.is_active_on(NaiveDate::from_ymd_opt(2017, 1, 7).unwrap()));

    // Verify StopTime has a resolved StopPlace::Stop and ShapeLinearRef
    let trip1 = validated
        .trips
        .get(&TripId::new("trip1"))
        .expect("trip1 should exist");
    assert_eq!(trip1.stop_times.len(), 3);
    assert!(trip1.stop_times[0].place.as_stop().is_some());
    assert!(trip1.stop_times[0].shape_ref.is_some());

    // Phase 3: RaptorTimetable
    let timetable =
        RaptorTimetable::try_from(validated).expect("Phase 3 RaptorTimetable should succeed");
    assert_eq!(timetable.places.len(), 6);
    assert_eq!(timetable.routes.len(), 4);
    assert_eq!(timetable.patterns.len(), 1);
    assert_eq!(timetable.trips.len(), 1);

    // Verify reverse lookup from PlaceIdx -> PatternIdx (stop2 is the first stop on trip1)
    let stop2_pidx = timetable.stop_id_to_place[&StopId::new("stop2")];
    let occurrences = timetable.patterns_serving_place(stop2_pidx);
    assert_eq!(occurrences.len(), 1);
    assert_eq!(occurrences[0].stop_offset_in_pattern, 0);

    // Verify transfers and pathways were indexed on PlaceIdx (stop1 has transfers & pathways)
    let stop1_pidx = timetable.stop_id_to_place[&StopId::new("stop1")];
    assert!(!timetable.place_to_transfers[stop1_pidx.index()].is_empty());

    // Verify DateMappingPolicy directly against the unaltered fixtures/basic archive:
    // - service1 in calendar.txt runs ONLY on Saturday & Sunday from 2017-01-01 to 2017-01-15,
    //   with 2017-01-01 (Sunday) and 2017-01-02 (Monday) deleted in calendar_dates.txt.
    // - service2 is Added on 2017-01-01 (Sunday) in calendar_dates.txt.
    let wed_in_range = NaiveDate::from_ymd_opt(2017, 1, 4).unwrap(); // Wednesday inside [2017-01-01, 2017-01-15]
    let sat_in_range = NaiveDate::from_ymd_opt(2017, 1, 7).unwrap(); // Saturday inside [2017-01-01, 2017-01-15]
    let sat_out_of_range = NaiveDate::from_ymd_opt(2017, 1, 21).unwrap(); // Saturday outside range (+7 days from 2017-01-14)
    let sat_2017_01_14 = NaiveDate::from_ymd_opt(2017, 1, 14).unwrap();

    // 1. Wednesday 2017-01-04 is inside [start_date, end_date], but basic has NO Wednesday service:
    //    - Exact and Nearest(match_weekday: true) must both return None (and 0 active services)
    assert_eq!(
        timetable.resolve_service_date_for_feed(
            FeedIdx::new(0),
            wed_in_range,
            DateMappingPolicy::Exact
        ),
        None
    );
    assert_eq!(
        timetable.resolve_service_date_for_feed(
            FeedIdx::new(0),
            wed_in_range,
            DateMappingPolicy::Nearest {
                tolerance_days: 365,
                match_weekday: true,
            }
        ),
        None
    );
    let wed_exact_win = timetable
        .active_services_for_feed_with_policy(
            FeedIdx::new(0),
            wed_in_range,
            bambam_gtfs::GtfsTime::from_hms(13, 0, 0),
            DateMappingPolicy::Nearest {
                tolerance_days: 365,
                match_weekday: true,
            },
        )
        .unwrap();
    assert!(timetable
        .earliest_boardable_trip_with_overnight(PatternIdx::new(0), 0, &wed_exact_win)
        .is_none());

    // 2. BestCase on Wednesday 2017-01-04 falls back to match_weekday: false and picks Sunday 2017-01-01 (3 days earlier, tie-breaker over Saturday 2017-01-07 which is 3 days later)
    assert_eq!(
        timetable.resolve_service_date_for_feed(
            FeedIdx::new(0),
            wed_in_range,
            DateMappingPolicy::BestCase {
                tolerance_days: 365
            }
        ),
        Some(NaiveDate::from_ymd_opt(2017, 1, 1).unwrap())
    );

    // 3. Saturday 2017-01-07 is an exact match; out-of-range Saturday 2017-01-21 maps to Saturday 2017-01-14
    assert_eq!(
        timetable.resolve_service_date_for_feed(
            FeedIdx::new(0),
            sat_in_range,
            DateMappingPolicy::Exact
        ),
        Some(sat_in_range)
    );
    let mapped_sat_win = timetable
        .active_services_for_feed_with_policy(
            FeedIdx::new(0),
            sat_out_of_range,
            bambam_gtfs::GtfsTime::from_hms(5, 30, 0),
            DateMappingPolicy::Nearest {
                tolerance_days: 30,
                match_weekday: true,
            },
        )
        .unwrap();
    assert_eq!(mapped_sat_win.date, sat_out_of_range);
    assert_eq!(mapped_sat_win.picked_date, sat_2017_01_14);
    let dep = timetable
        .earliest_boardable_trip_with_overnight(PatternIdx::new(0), 0, &mapped_sat_win)
        .expect("trip1 should be boardable on mapped Saturday 2017-01-14");
    assert_eq!(
        dep.departure_datetime(&timetable, mapped_sat_win.date)
            .unwrap()
            .date_naive(),
        sat_out_of_range
    );
}

#[test]
fn test_zip_archive_reading() {
    let timetable = RaptorTimetable::from_path(fixture_path("fixtures/zips/gtfs.zip"))
        .expect("should read zip archive");
    assert_eq!(timetable.places.len(), 5);
    assert!(!timetable.routes.is_empty());
    assert_eq!(timetable.trips.len(), 1);

    let leg_coords = timetable
        .reconstruct_leg_linestring(TripIdx::new(0), 0, 1)
        .expect("should reconstruct leg linestring");
    assert!(leg_coords.len() >= 2);
}

#[test]
fn test_compact_memory_layout_and_multi_feed_merger() {
    // 1. Verify exact byte sizes for HPC continental-scale memory budget
    assert_eq!(
        std::mem::size_of::<CompactStopTime>(),
        16,
        "CompactStopTime must be exactly 16 bytes (4 per 64B cache line)"
    );
    assert_eq!(
        std::mem::size_of::<ShapeVertex>(),
        12,
        "ShapeVertex must be 12 bytes (f32 lon, lat, cumulative_meters)"
    );

    // 2. Merge two feeds that have overlapping local IDs ("stop1", "stop2", "route1", "trip1")
    let feed_a = ValidatedFeed::from_path(fixture_path("fixtures/basic")).expect("load feed A");
    let feed_b = ValidatedFeed::from_path(fixture_path("fixtures/interpolated_stop_times"))
        .expect("load feed B");

    let mut builder = RaptorTimetable::builder();
    let fidx_a = builder
        .add_feed("agency_alpha", feed_a)
        .expect("add feed A");
    let fidx_b = builder.add_feed("agency_beta", feed_b).expect("add feed B");
    assert_eq!(fidx_a, FeedIdx::new(0));
    assert_eq!(fidx_b, FeedIdx::new(1));

    let merged = builder.build();
    assert_eq!(merged.feeds.len(), 2);
    assert_eq!(merged.trips.len(), 2);

    // 3. Forward lookup: both feeds have "trip1" and "stop2", scoped lookup disambiguates them!
    let alpha_id = FeedId::new("agency_alpha");
    let beta_id = FeedId::new("agency_beta");
    let trip1_id = TripId::new("trip1");
    let stop2_id = StopId::new("stop2");

    let tidx_a = merged
        .lookup_trip(&alpha_id, &trip1_id)
        .expect("lookup alpha:trip1");
    let tidx_b = merged
        .lookup_trip(&beta_id, &trip1_id)
        .expect("lookup beta:trip1");
    assert_ne!(tidx_a, tidx_b);

    let pidx_a = merged
        .lookup_stop(&alpha_id, &stop2_id)
        .expect("lookup alpha:stop2");
    let pidx_b = merged
        .lookup_stop(&beta_id, &stop2_id)
        .expect("lookup beta:stop2");
    assert_ne!(pidx_a, pidx_b);

    // 4. O(1) Reverse lookup: Global Index -> Fully-Qualified Lineage (FeedId, AgencyId, RouteId, TripId, StopId, Tz)
    let fq_trip_a = merged.resolve_trip_id(tidx_a).expect("resolve tidx_a");
    assert_eq!(fq_trip_a.feed_id.as_str(), "agency_alpha");
    assert_eq!(fq_trip_a.route_id.as_str(), "route1");
    assert_eq!(fq_trip_a.trip_id.as_str(), "trip1");
    assert_eq!(fq_trip_a.timezone, Some(chrono_tz::Tz::Europe__Paris));

    let fq_trip_b = merged.resolve_trip_id(tidx_b).expect("resolve tidx_b");
    assert_eq!(fq_trip_b.feed_id.as_str(), "agency_beta");
    assert_eq!(fq_trip_b.agency_name, Some("BIBUS"));
    assert_eq!(fq_trip_b.route_id.as_str(), "route1");
    assert_eq!(fq_trip_b.trip_id.as_str(), "trip1");
    assert_eq!(fq_trip_b.timezone, Some(chrono_tz::Tz::Europe__Paris));

    let fq_place_a = merged.resolve_place_id(pidx_a).expect("resolve pidx_a");
    assert_eq!(fq_place_a.feed_id.as_str(), "agency_alpha");
    assert_eq!(fq_place_a.place_id.as_str(), "stop2");

    let fq_place_b = merged.resolve_place_id(pidx_b).expect("resolve pidx_b");
    assert_eq!(fq_place_b.feed_id.as_str(), "agency_beta");
    assert_eq!(fq_place_b.place_id.as_str(), "stop2");
}

#[cfg(feature = "gtfs-flex")]
#[test]
fn test_gtfs_flex_phased_pipeline_and_location_groups() {
    // 0. Standalone FlexTables bundle loading without loading the rest of the feed
    let mut flex_archive =
        GtfsArchive::open(fixture_path("fixtures/flex")).expect("open flex archive");
    let flex_only: FlexTables = flex_archive
        .load()
        .expect("should load standalone FlexTables bundle");
    assert_eq!(flex_only.locations.as_ref().map(Vec::len), Some(2));
    assert_eq!(flex_only.booking_rules.as_ref().map(Vec::len), Some(1));

    // 1. Read the existing fixtures/flex feed through all 3 phases
    let timetable = RaptorTimetable::from_path(fixture_path("fixtures/flex"))
        .expect("fixtures/flex should pass Phases 1, 2, and 3");
    assert_eq!(timetable.location_id_to_place.len(), 2);
    assert!(!timetable.trips.is_empty());

    // Verify holiday exception + date mapping on unaltered fixtures/flex:
    // - c_67295_b_77497_d_31 (Weekday) has 2024-07-04 (Thursday, Independence Day) deleted in calendar_dates.txt.
    // - Querying on Thursday 2024-07-04 under Nearest(match_weekday: true) skips the holiday and maps to Thursday 2024-06-27!
    let july_4_2024 = NaiveDate::from_ymd_opt(2024, 7, 4).unwrap();
    let june_27_2024 = NaiveDate::from_ymd_opt(2024, 6, 27).unwrap();
    assert_eq!(
        timetable.resolve_service_date_for_feed(
            FeedIdx::new(0),
            july_4_2024,
            DateMappingPolicy::Exact
        ),
        None
    );
    assert_eq!(
        timetable.resolve_service_date_for_feed(
            FeedIdx::new(0),
            july_4_2024,
            DateMappingPolicy::Nearest {
                tolerance_days: 30,
                match_weekday: true,
            }
        ),
        Some(june_27_2024)
    );
    // And on Wednesday 2024-07-03 (in range, non-holiday), only the Weekday service is active (Sunday service is false)
    let wed_july_3 = NaiveDate::from_ymd_opt(2024, 7, 3).unwrap();
    let flex_wed_win = timetable
        .active_services_for_feed_with_policy(
            FeedIdx::new(0),
            wed_july_3,
            GtfsTime::from_hms(7, 0, 0),
            DateMappingPolicy::BestCase {
                tolerance_days: 730,
            },
        )
        .unwrap();
    assert_eq!(flex_wed_win.picked_date, wed_july_3);
    assert_eq!(
        flex_wed_win.today_active.iter().filter(|&&b| b).count(),
        1,
        "only Weekday service should be active on Wednesday, not Sunday service"
    );

    let first_st = timetable.trip_stop_times(TripIdx::new(0))[0];
    assert!(first_st.flex_window().is_some());
    assert_eq!(
        first_st.flex_window().unwrap().start,
        GtfsTime::from_hms(6, 15, 0)
    );
    assert_eq!(
        first_st.flex_window().unwrap().end,
        GtfsTime::from_hms(8, 0, 0)
    );
    let flex_dur = timetable.stop_time_flex_duration(first_st);
    assert!((flex_dur.estimate_mean_seconds(600.0) - 2400.0).abs() < 1e-6);

    // 2. Construct a synthetic RawFeed exercising LocationGroup, MultiPolygon, BookingRule, and Spatial traits
    let mut raw = RawFeed::from_path(fixture_path("fixtures/flex")).unwrap();
    raw.location_groups = Some(vec![RawLocationGroup {
        location_group_id: LocationGroupId::new("lg_1"),
        location_group_name: Some("Downtown & Area 708 Group".to_owned()),
    }]);
    raw.location_group_stops = Some(vec![RawLocationGroupStop {
        location_group_id: LocationGroupId::new("lg_1"),
        stop_id: "area_708".to_owned(),
    }]);
    // Point the first stop_time at location_group_id = "lg_1" instead of location_id
    raw.stop_times[0].location_id = None;
    raw.stop_times[0].location_group_id = Some(LocationGroupId::new("lg_1"));

    let validated = ValidatedFeed::try_from(raw).expect("location group validation should succeed");
    let lg = validated
        .location_groups
        .get(&LocationGroupId::new("lg_1"))
        .expect("lg_1 should be present");
    assert_eq!(lg.id().as_str(), "lg_1");
    assert_eq!(lg.members.len(), 1);
    assert!(lg.bounding_box().is_some());
    assert!(lg.representative_coord().is_some());

    let flex_timetable =
        RaptorTimetable::try_from(validated).expect("Phase 3 with location group should succeed");
    let lg_pidx = flex_timetable.location_group_id_to_place[&LocationGroupId::new("lg_1")];
    let area_708_pidx = flex_timetable.location_id_to_place[&LocationId::new("area_708")];

    // Both the group's PlaceIdx AND its member area_708's PlaceIdx should index the pattern!
    assert!(!flex_timetable.patterns_serving_place(lg_pidx).is_empty());
    assert!(!flex_timetable
        .patterns_serving_place(area_708_pidx)
        .is_empty());

    // 3. Zone-only GTFS-Flex archive without `stops.txt`
    let mut zip_buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut zip_buf);
        let opts = zip::write::SimpleFileOptions::default();
        for entry in std::fs::read_dir(fixture_path("fixtures/flex")).unwrap() {
            let entry = entry.unwrap();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name == "stops.txt" {
                continue;
            }
            let bytes = std::fs::read(entry.path()).unwrap();
            std::io::Write::write_all(
                &mut {
                    zw.start_file(&file_name, opts).unwrap();
                    &mut zw
                },
                &bytes,
            )
            .unwrap();
        }
        zw.finish().unwrap();
    }

    zip_buf.set_position(0);
    let zone_only_raw = RawFeed::from_reader(zip_buf.clone())
        .expect("zone-only RawFeed::from_reader should succeed");
    assert!(zone_only_raw.stops.is_empty());

    let zone_only_via_reader = RawFeedReader::default()
        .read_from_reader(zip_buf)
        .expect("zone-only RawFeedReader::read_from_reader should succeed");
    assert!(zone_only_via_reader.stops.is_empty());

    let zone_only_validated = ValidatedFeed::try_from(zone_only_raw)
        .expect("zone-only ValidatedFeed validation should succeed");
    assert!(zone_only_validated.stops.is_empty());
    assert_eq!(zone_only_validated.locations.len(), 2);

    // Fixed-stop reference in a zone-only feed without stops.txt fails validation cleanly
    let mut bad_zone_only = zone_only_via_reader;
    bad_zone_only.stop_times[0].location_id = None;
    bad_zone_only.stop_times[0].stop_id = Some(StopId::new("missing_fixed_stop"));
    assert!(matches!(
        ValidatedFeed::try_from(bad_zone_only),
        Err(ModelError::MissingReference {
            source_file: "stop_times.txt",
            field_name: "stop_id",
            ..
        })
    ));
}
