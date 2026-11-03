//! Unit and integration tests for `gtfs2` across all three initialization phases,
//! GTFS-Flex extensions, and shape linear referencing / sub-segment extraction.

use crate::ids::*;
use crate::model::*;
#[cfg(feature = "gtfs-flex")]
use crate::primitives::GtfsTime;
use crate::raw::*;
use crate::routing::*;
use crate::traits::TemporalStopTime;
#[cfg(feature = "gtfs-flex")]
use crate::traits::{Identifiable, Spatial};
use chrono::NaiveDate;
use std::convert::TryFrom;

#[test]
fn test_id_newtypes_and_indices() {
    let stop_id = StopId::new("stop_1");
    assert_eq!(stop_id.as_str(), "stop_1");
    assert_eq!(&*stop_id, "stop_1");
    assert_eq!(format!("{stop_id}"), "stop_1");

    let place_idx = PlaceIdx::from_usize(42);
    assert_eq!(place_idx.index(), 42);
    assert_eq!(place_idx.as_u32(), 42);
}

#[test]
fn test_phased_pipeline_basic_fixture() {
    // Phase 1: RawFeed
    let mut raw = RawFeed::from_path("fixtures/basic").expect("Phase 1 RawFeed should succeed");
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

    // Phase 3: RoutingTimetable
    let timetable =
        RoutingTimetable::try_from(validated).expect("Phase 3 RoutingTimetable should succeed");
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
}

#[test]
fn test_interpolated_stop_times_in_phase_3() {
    // Notice: this single test compiles and passes both WITH and WITHOUT `--features gtfs-flex`!
    let validated = ValidatedFeed::from_path("fixtures/interpolated_stop_times")
        .expect("Phase 2 should accept uninterpolated intermediate stop times");
    let trip1 = &validated.trips[&TripId::new("trip1")];
    assert!(matches!(
        trip1.stop_times[1].timing,
        StopTimeTiming::Uninterpolated {
            arrival: None,
            departure: None
        }
    ));

    // Phase 3 interpolates the missing intermediate stop time between 14:00:00 and 15:00:00
    let timetable =
        RoutingTimetable::try_from(validated).expect("Phase 3 should interpolate stop times");
    let st_slice = timetable.trip_stop_times(TripIdx::new(0));
    assert_eq!(st_slice.len(), 3);

    let st0_dep = st_slice[0].departure_time().unwrap();
    let st1_arr = st_slice[1].arrival_time().unwrap();
    let st2_arr = st_slice[2].arrival_time().unwrap();
    assert!(
        st1_arr > st0_dep && st1_arr < st2_arr,
        "interpolated time {st1_arr} should be strictly between {st0_dep} and {st2_arr}"
    );
}

#[test]
fn test_shape_linear_referencing_and_subsegment_slicing() {
    let raw_points = vec![
        RawShapePoint {
            id: ShapeId::new("shp1"),
            latitude: 40.0,
            longitude: -105.0,
            sequence: 1,
            dist_traveled: Some(0.0),
        },
        RawShapePoint {
            id: ShapeId::new("shp1"),
            latitude: 40.0,
            longitude: -104.95,
            sequence: 2,
            dist_traveled: Some(50.0),
        },
        RawShapePoint {
            id: ShapeId::new("shp1"),
            latitude: 40.05,
            longitude: -104.95,
            sequence: 3,
            dist_traveled: Some(100.0),
        },
    ];

    let poly = ShapePolyline::from_raw_points(ShapeId::new("shp1"), raw_points)
        .expect("valid shape polyline");
    assert_eq!(poly.vertices.len(), 3);
    assert!(poly.total_meters > 8_000.0);

    // Resolve raw_dist = 25.0 (halfway along the first segment) and raw_dist = 75.0 (halfway along the second segment)
    let ref_start = poly.resolve_raw_dist(25.0).unwrap();
    let ref_end = poly.resolve_raw_dist(75.0).unwrap();
    assert!(ref_start.arc_meters < poly.vertices[1].cumulative_meters as f64);
    assert!(ref_end.arc_meters > poly.vertices[1].cumulative_meters as f64);

    // Slice the sub-segment linestring from ref_start to ref_end:
    // It should contain [interpolated_start, corner_vertex_2, interpolated_end] = 3 coordinates!
    let sub_linestring = poly.slice_linestring(ref_start.arc_meters, ref_end.arc_meters);
    assert_eq!(sub_linestring.len(), 3);
    assert!((sub_linestring[0].lon - (-104.975)).abs() < 1e-4);
    assert!((sub_linestring[1].lon - (-104.95)).abs() < 1e-5);
    assert!((sub_linestring[1].lat - 40.0).abs() < 1e-5);
    assert!((sub_linestring[2].lat - 40.025).abs() < 1e-4);
}

#[test]
fn test_zip_archive_reading() {
    let timetable =
        RoutingTimetable::from_path("fixtures/zips/gtfs.zip").expect("should read zip archive");
    assert_eq!(timetable.places.len(), 5);
    assert!(!timetable.routes.is_empty());
    assert_eq!(timetable.trips.len(), 1);

    let leg_coords = timetable
        .reconstruct_leg_linestring(TripIdx::new(0), 0, 1)
        .expect("should reconstruct leg linestring");
    assert!(leg_coords.len() >= 2);
}

#[test]
fn test_selective_table_loading_and_streaming() {
    // 1. Selective tuple loading from a zip archive (never touches stop_times.txt or shapes.txt!)
    let mut archive = GtfsArchive::open("fixtures/zips/gtfs.zip").expect("should open zip archive");
    assert!(archive.contains_file("routes.txt"));
    assert!(!archive.contains_file("nonexistent_table.txt"));

    let (routes, calendar, levels): (
        Vec<RawRoute>,
        Option<Vec<RawCalendar>>,
        Option<Vec<RawLevel>>,
    ) = archive
        .load()
        .expect("should selectively load tuple of required and optional tables");
    assert_eq!(routes.len(), 1);
    assert_eq!(calendar.expect("calendar.txt present").len(), 1);
    assert!(levels.is_none());

    // 2. Streaming O(1)-memory iterator over stop_times.txt
    let stop_time_count = archive
        .table_iter::<RawStopTime>()
        .expect("should open streaming CSV iterator")
        .map(|r| r.expect("valid stop_time row"))
        .count();
    assert_eq!(stop_time_count, 2);

    // 3. Custom user-defined table loaded via GtfsTable without modifying reader.rs
    #[derive(Debug, Deserialize)]
    struct CustomLicenseRow {
        #[serde(rename = "license_spdx_id")]
        spdx_id: Option<String>,
    }
    crate::impl_gtfs_csv_table!(CustomLicenseRow, "licenses.txt", mandatory = false);

    let mut dir_archive =
        GtfsArchive::open("fixtures/basic").expect("should open directory archive");
    let licenses: Option<Vec<CustomLicenseRow>> = dir_archive
        .load()
        .expect("should load custom extension table");
    assert!(licenses.is_some());
    assert!(!licenses.unwrap().is_empty());
    assert!(dir_archive
        .load::<Vec<CustomLicenseRow>>()
        .unwrap()
        .iter()
        .any(|l| l.spdx_id.is_some()));
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
    let feed_a = ValidatedFeed::from_path("fixtures/basic").expect("load feed A");
    let feed_b = ValidatedFeed::from_path("fixtures/interpolated_stop_times").expect("load feed B");

    let mut builder = RoutingTimetable::builder();
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

    // 4. O(1) Reverse lookup: Global Index -> Fully-Qualified Lineage (FeedId, AgencyId, RouteId, TripId, StopId)
    let fq_trip_a = merged.resolve_trip_id(tidx_a).expect("resolve tidx_a");
    assert_eq!(fq_trip_a.feed_id.as_str(), "agency_alpha");
    assert_eq!(fq_trip_a.route_id.as_str(), "route1");
    assert_eq!(fq_trip_a.trip_id.as_str(), "trip1");

    let fq_trip_b = merged.resolve_trip_id(tidx_b).expect("resolve tidx_b");
    assert_eq!(fq_trip_b.feed_id.as_str(), "agency_beta");
    assert_eq!(fq_trip_b.agency_name, Some("BIBUS"));
    assert_eq!(fq_trip_b.route_id.as_str(), "route1");
    assert_eq!(fq_trip_b.trip_id.as_str(), "trip1");

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
    let mut flex_archive = GtfsArchive::open("fixtures/flex").expect("open flex archive");
    let flex_only: FlexTables = flex_archive
        .load()
        .expect("should load standalone FlexTables bundle");
    assert_eq!(flex_only.locations.as_ref().map(Vec::len), Some(2));
    assert_eq!(flex_only.booking_rules.as_ref().map(Vec::len), Some(1));

    // 1. Read the existing fixtures/flex feed through all 3 phases
    let timetable = RoutingTimetable::from_path("fixtures/flex")
        .expect("fixtures/flex should pass Phases 1, 2, and 3");
    assert_eq!(timetable.location_id_to_place.len(), 2);
    assert!(!timetable.trips.is_empty());

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
    let mut raw = RawFeed::from_path("fixtures/flex").unwrap();
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
        RoutingTimetable::try_from(validated).expect("Phase 3 with location group should succeed");
    let lg_pidx = flex_timetable.location_group_id_to_place[&LocationGroupId::new("lg_1")];
    let area_708_pidx = flex_timetable.location_id_to_place[&LocationId::new("area_708")];

    // Both the group's PlaceIdx AND its member area_708's PlaceIdx should index the pattern!
    assert!(!flex_timetable.patterns_serving_place(lg_pidx).is_empty());
    assert!(!flex_timetable
        .patterns_serving_place(area_708_pidx)
        .is_empty());
}

#[cfg(feature = "gtfs-flex")]
#[test]
fn test_gtfs_flex_validation_errors() {
    // 1. Namespace collision between stops.txt and locations.geojson
    let mut raw = RawFeed::from_path("fixtures/flex").unwrap();
    raw.stops.push(RawStop {
        id: StopId::new("area_708"),
        name: Some("Conflicting Stop".to_owned()),
        ..Default::default()
    });
    let err = ValidatedFeed::try_from(raw).unwrap_err();
    assert!(matches!(err, ModelError::NamespaceCollision { id } if id == "area_708"));

    // 2. Mutually exclusive stop_id and location_id on the same stop_time
    let mut raw2 = RawFeed::from_path("fixtures/flex").unwrap();
    raw2.stops.push(RawStop {
        id: StopId::new("valid_stop_1"),
        ..Default::default()
    });
    raw2.stop_times[0].stop_id = Some(StopId::new("valid_stop_1"));
    // location_id is already Some("area_715")
    let err2 = ValidatedFeed::try_from(raw2).unwrap_err();
    assert!(matches!(
        err2,
        ModelError::MutuallyExclusivePlaceFields { .. }
    ));
}
