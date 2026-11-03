//! Unit and integration tests for `gtfs2` across all three initialization phases,
//! GTFS-Flex extensions, and shape linear referencing / sub-segment extraction.

use crate::ids::*;
use crate::model::*;
#[cfg(feature = "gtfs-flex")]
use crate::primitives::Coord;
use crate::primitives::GtfsTime;
use crate::raw::*;
use crate::routing::*;
#[cfg(feature = "gtfs-flex")]
use crate::traits::Identifiable;
use crate::traits::{Spatial, TemporalStopTime};
use chrono::NaiveDate;
use std::convert::TryFrom;
use std::path::PathBuf;

fn fixture_path(rel_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path)
}

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
    let validated = ValidatedFeed::from_path(fixture_path("fixtures/interpolated_stop_times"))
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
    let timetable = RoutingTimetable::from_path(fixture_path("fixtures/zips/gtfs.zip"))
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
fn test_selective_table_loading_and_streaming() {
    // 1. Selective tuple loading from a zip archive (never touches stop_times.txt or shapes.txt!)
    let mut archive =
        GtfsArchive::open(fixture_path("fixtures/zips/gtfs.zip")).expect("should open zip archive");
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
        .filter(|r| r.is_ok())
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
        GtfsArchive::open(fixture_path("fixtures/basic")).expect("should open directory archive");
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
    let feed_a = ValidatedFeed::from_path(fixture_path("fixtures/basic")).expect("load feed A");
    let feed_b = ValidatedFeed::from_path(fixture_path("fixtures/interpolated_stop_times"))
        .expect("load feed B");

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
    let mut flex_archive =
        GtfsArchive::open(fixture_path("fixtures/flex")).expect("open flex archive");
    let flex_only: FlexTables = flex_archive
        .load()
        .expect("should load standalone FlexTables bundle");
    assert_eq!(flex_only.locations.as_ref().map(Vec::len), Some(2));
    assert_eq!(flex_only.booking_rules.as_ref().map(Vec::len), Some(1));

    // 1. Read the existing fixtures/flex feed through all 3 phases
    let timetable = RoutingTimetable::from_path(fixture_path("fixtures/flex"))
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
    let mut raw = RawFeed::from_path(fixture_path("fixtures/flex")).unwrap();
    raw.stops.push(RawStop {
        id: StopId::new("area_708"),
        name: Some("Conflicting Stop".to_owned()),
        ..Default::default()
    });
    let err = ValidatedFeed::try_from(raw).unwrap_err();
    assert!(matches!(err, ModelError::NamespaceCollision { id } if id == "area_708"));

    // 2. Mutually exclusive stop_id and location_id on the same stop_time
    let mut raw2 = RawFeed::from_path(fixture_path("fixtures/flex")).unwrap();
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

#[test]
fn test_frequency_trips_earliest_boardable_and_materialization() {
    let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
    // Configure a 1-hour frequency window [06:00:00, 07:00:00) with 600s (10-minute) headways
    // on trip1 (whose template stop_times are: stop2 @ 14:00:00, stop3 @ 15:00:00 (+3600s), stop4 @ 16:00:00 (+7200s))
    raw.frequencies = Some(vec![RawFrequency {
        trip_id: TripId::new("trip1"),
        start_time: GtfsTime::from_hms(6, 0, 0),
        end_time: GtfsTime::from_hms(7, 0, 0),
        headway_secs: 600,
        exact_times: crate::enums::TimepointType::Approximate,
    }]);
    let validated = ValidatedFeed::try_from(raw).unwrap();

    // 1. Test non-materialized frequency query via earliest_boardable_trip
    let timetable = RoutingTimetable::try_from(validated.clone()).unwrap();
    let active_mask = vec![true; timetable.services.len()];
    let pat_idx = PatternIdx::new(0);

    // At stop_offset = 0 (stop2), querying at 06:07:00 should board the 06:10:00 headway departure
    let dep0 = timetable
        .earliest_boardable_trip(pat_idx, 0, GtfsTime::from_hms(6, 7, 0), &active_mask)
        .expect("should find frequency departure at stop 0");
    assert_eq!(dep0.departure_time, GtfsTime::from_hms(6, 10, 0));
    // Arrival at stop_offset = 2 (stop4, +2 hours from stop 0) should be 08:10:00
    assert_eq!(
        dep0.arrival_time_at(&timetable, 2),
        Some(GtfsTime::from_hms(8, 10, 0))
    );

    // At stop_offset = 1 (stop3, +1 hour from stop 0), querying at 07:12:00:
    // The 06:10:00 vehicle at stop 0 reaches stop 1 at 07:10:00 (too early).
    // The 06:20:00 vehicle at stop 0 reaches stop 1 at 07:20:00!
    let dep1 = timetable
        .earliest_boardable_trip(pat_idx, 1, GtfsTime::from_hms(7, 12, 0), &active_mask)
        .expect("should find frequency departure at stop 1");
    assert_eq!(dep1.departure_time, GtfsTime::from_hms(7, 20, 0));
    assert_eq!(
        dep1.arrival_time_at(&timetable, 2),
        Some(GtfsTime::from_hms(8, 20, 0))
    );

    // 2. Test materialized frequency trips in RoutingTimetableBuilder
    let mut builder = RoutingTimetable::builder().with_materialize_frequencies(true);
    builder.add_feed("basic_mat", validated).unwrap();
    let mat_timetable = builder.build();

    // Window [06:00:00, 07:00:00) every 600s -> 6 concrete trips (06:00, 06:10, 06:20, 06:30, 06:40, 06:50)
    assert_eq!(mat_timetable.trips.len(), 6);
    assert!(mat_timetable.trips.iter().all(|t| t.frequencies.is_empty()));

    let mat_active = vec![true; mat_timetable.services.len()];
    let mat_dep0 = mat_timetable
        .earliest_boardable_trip(pat_idx, 0, GtfsTime::from_hms(6, 7, 0), &mat_active)
        .expect("should binary-search materialized frequency trips");
    assert_eq!(mat_dep0.departure_time, GtfsTime::from_hms(6, 10, 0));
    assert_eq!(mat_dep0.frequency_offset_secs, 0);
    assert_eq!(
        mat_dep0.arrival_time_at(&mat_timetable, 2),
        Some(GtfsTime::from_hms(8, 10, 0))
    );
}

#[cfg(feature = "gtfs-flex")]
#[test]
fn test_point_in_polygon_and_rtree_spatial_index() {
    // Build an L-shaped polygon where the bounding box center (0.75, 0.75) is OUTSIDE the L!
    // L-shape vertices: (0,0) -> (1,0) -> (1,0.4) -> (0.4,0.4) -> (0.4,1) -> (0,1) -> (0,0)
    let l_ring = vec![
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.0),
        Coord::new(1.0, 0.4),
        Coord::new(0.4, 0.4),
        Coord::new(0.4, 1.0),
        Coord::new(0.0, 1.0),
        Coord::new(0.0, 0.0),
    ];
    let raw_loc = RawFlexLocation {
        id: LocationId::new("l_zone"),
        stop_name: Some("L-Shaped Flex Zone".to_owned()),
        stop_desc: None,
        zone_id: None,
        stop_url: None,
        geometry: RawFlexGeometry::Polygon(vec![l_ring]),
    };
    let flex_loc = FlexLocation::from(raw_loc.clone());

    let inside_bottom = Coord::new(0.8, 0.2);
    let inside_left = Coord::new(0.2, 0.8);
    let outside_in_bbox_cutout = Coord::new(0.8, 0.8);
    let completely_outside = Coord::new(2.0, 2.0);

    assert!(flex_loc.contains_coord(inside_bottom));
    assert!(flex_loc.contains_coord(inside_left));
    assert!(
        flex_loc.bbox.unwrap().contains(outside_in_bbox_cutout),
        "cutout point is inside the bounding box"
    );
    assert!(
        !flex_loc.contains_coord(outside_in_bbox_cutout),
        "exact ray-casting must reject point in L-shape cutout"
    );
    assert!(!flex_loc.contains_coord(completely_outside));

    // Verify RoutingTimetable R-tree + exact point-in-polygon filtering in places_near
    let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
    raw.locations = Some(vec![raw_loc]);
    let validated = ValidatedFeed::try_from(raw).unwrap();
    let timetable = RoutingTimetable::try_from(validated).unwrap();

    let l_zone_pidx = timetable.location_id_to_place[&LocationId::new("l_zone")];
    assert!(timetable
        .places_near(inside_bottom, 0.0)
        .contains(&l_zone_pidx));
    assert!(
        !timetable
            .places_near(outside_in_bbox_cutout, 0.0)
            .contains(&l_zone_pidx),
        "places_near(radius=0) should not match L-zone when point is in the cutout"
    );
}

#[test]
fn test_lenient_validation_policy_dirty_gtfs() {
    let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();

    // Inject dirty GTFS anomalies:
    // 1. A trip with 0 stop_times rows
    raw.trips.push(RawTrip {
        id: TripId::new("dirty_empty_trip"),
        service_id: ServiceId::new("service1"),
        route_id: RouteId::new("route1"),
        ..Default::default()
    });
    // 2. A trip with only 1 stop_times row
    raw.trips.push(RawTrip {
        id: TripId::new("dirty_single_stop_trip"),
        service_id: ServiceId::new("service1"),
        route_id: RouteId::new("route1"),
        ..Default::default()
    });
    raw.stop_times.push(RawStopTime {
        trip_id: TripId::new("dirty_single_stop_trip"),
        stop_id: Some(StopId::new("stop1")),
        stop_sequence: 1,
        arrival_time: Some(GtfsTime::from_hms(8, 0, 0)),
        departure_time: Some(GtfsTime::from_hms(8, 0, 0)),
        ..Default::default()
    });
    // 3. A trip with decreasing stop times (10:05:00 followed by 10:04:00)
    raw.trips.push(RawTrip {
        id: TripId::new("dirty_decreasing_trip"),
        service_id: ServiceId::new("service1"),
        route_id: RouteId::new("route1"),
        ..Default::default()
    });
    raw.stop_times.push(RawStopTime {
        trip_id: TripId::new("dirty_decreasing_trip"),
        stop_id: Some(StopId::new("stop1")),
        stop_sequence: 1,
        arrival_time: Some(GtfsTime::from_hms(10, 5, 0)),
        departure_time: Some(GtfsTime::from_hms(10, 5, 0)),
        ..Default::default()
    });
    raw.stop_times.push(RawStopTime {
        trip_id: TripId::new("dirty_decreasing_trip"),
        stop_id: Some(StopId::new("stop2")),
        stop_sequence: 2,
        arrival_time: Some(GtfsTime::from_hms(10, 4, 0)),
        departure_time: Some(GtfsTime::from_hms(10, 4, 0)),
        ..Default::default()
    });
    // 4. Duplicate stop_sequence on valid trip1
    let dup_st = raw.stop_times[0].clone();
    raw.stop_times.push(dup_st);
    // 5. Orphan stop_time referencing non-existent trip
    raw.stop_times.push(RawStopTime {
        trip_id: TripId::new("ghost_trip"),
        stop_id: Some(StopId::new("stop1")),
        stop_sequence: 1,
        ..Default::default()
    });

    // Strict mode must reject this dirty feed
    assert!(ValidatedFeed::from_raw_with_policy(raw.clone(), ValidationPolicy::Strict).is_err());

    // Lenient mode succeeds in Phase 2 and Phase 3, preserving valid trip1 and recording warnings
    let validated =
        ValidatedFeed::from_raw_with_policy(raw, ValidationPolicy::DropInvalidTripsWithWarning)
            .expect("Phase 2 lenient validation should succeed");
    assert_eq!(validated.dropped_counts.empty_trips, 1);
    assert_eq!(validated.dropped_counts.single_stop_trips, 1);
    assert_eq!(validated.dropped_counts.duplicate_stop_sequences, 1);
    assert_eq!(validated.dropped_counts.orphan_stop_times, 1);

    let mut builder = RoutingTimetable::builder()
        .with_validation_policy(ValidationPolicy::DropInvalidTripsWithWarning);
    builder
        .add_feed("dirty_feed", validated)
        .expect("Phase 3 lenient build should succeed");
    let timetable = builder.build();

    assert_eq!(timetable.trips.len(), 1);
    assert_eq!(timetable.trips[0].id.as_str(), "trip1");
    let feed_meta = &timetable.feeds[0];
    assert_eq!(feed_meta.dropped_counts.empty_trips, 1);
    assert_eq!(feed_meta.dropped_counts.single_stop_trips, 1);
    assert_eq!(feed_meta.dropped_counts.malformed_trips, 1);
    assert_eq!(feed_meta.dropped_counts.total_dropped_trips(), 3);
    assert!(!feed_meta.warnings.is_empty());
}

#[test]
fn test_timezone_and_overnight_service_day_normalization() {
    let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
    // Add an overnight trip on service1 departing at 24:30:00 (00:30 AM next calendar day)
    raw.trips.push(RawTrip {
        id: TripId::new("overnight_trip"),
        service_id: ServiceId::new("service1"),
        route_id: RouteId::new("route1"),
        ..Default::default()
    });
    raw.stop_times.push(RawStopTime {
        trip_id: TripId::new("overnight_trip"),
        stop_id: Some(StopId::new("stop2")),
        stop_sequence: 1,
        arrival_time: Some(GtfsTime::from_hms(24, 30, 0)),
        departure_time: Some(GtfsTime::from_hms(24, 30, 0)),
        ..Default::default()
    });
    raw.stop_times.push(RawStopTime {
        trip_id: TripId::new("overnight_trip"),
        stop_id: Some(StopId::new("stop3")),
        stop_sequence: 2,
        arrival_time: Some(GtfsTime::from_hms(24, 40, 0)),
        departure_time: Some(GtfsTime::from_hms(24, 40, 0)),
        ..Default::default()
    });
    raw.stop_times.push(RawStopTime {
        trip_id: TripId::new("overnight_trip"),
        stop_id: Some(StopId::new("stop4")),
        stop_sequence: 3,
        arrival_time: Some(GtfsTime::from_hms(24, 50, 0)),
        departure_time: Some(GtfsTime::from_hms(24, 50, 0)),
        ..Default::default()
    });
    // Remove frequencies so trip1 is just a 14:00:00 trip
    raw.frequencies = None;

    let validated = ValidatedFeed::try_from(raw).unwrap();
    // Verify Agency parsed jiff::tz::TimeZone ("Europe/Paris" in fixtures/basic/agency.txt)
    assert_eq!(
        validated.agencies[0]
            .time_zone()
            .and_then(|tz| tz.iana_name()),
        Some("Europe/Paris")
    );

    let timetable = RoutingTimetable::try_from(validated).unwrap();
    assert_eq!(
        timetable.feeds[0]
            .primary_timezone
            .as_ref()
            .and_then(|tz| tz.iana_name()),
        Some("Europe/Paris")
    );

    // Suppose a rider searches on Monday 2017-01-09 at 00:15:00 local time.
    // Sunday 2017-01-08 (yesterday) had `service1` active, and `overnight_trip` departs at 24:30:00
    // on yesterday's clock (= 00:30:00 on Monday morning!).
    let query_date = NaiveDate::from_ymd_opt(2017, 1, 9).unwrap();
    let query_time = GtfsTime::from_hms(0, 15, 0);
    let window = timetable.active_services_at_time(query_date, query_time);
    assert_eq!(window.yesterday_overnight_time, GtfsTime::from_hms(24, 15, 0));

    let board = timetable
        .earliest_boardable_trip_with_overnight(PatternIdx::new(0), 0, &window)
        .expect("should board yesterday's overnight trip at 00:30 AM");
    assert!(board.is_overnight_from_yesterday);
    assert_eq!(board.departure_time, GtfsTime::from_hms(24, 30, 0));
    assert_eq!(
        board.normalized_query_day_departure(),
        GtfsTime::from_hms(0, 30, 0)
    );
}

#[test]
fn test_rkyv_snapshot_roundtrip() {
    let feed = ValidatedFeed::from_path(fixture_path("fixtures/basic")).unwrap();
    let original = RoutingTimetable::try_from(feed).unwrap();

    let bytes = original
        .to_rkyv_bytes()
        .expect("should serialize RoutingTimetable to rkyv");
    assert!(!bytes.is_empty());

    let restored = RoutingTimetable::from_rkyv_bytes(&bytes)
        .expect("should deserialize RoutingTimetable from rkyv");

    assert_eq!(restored.feeds, original.feeds);
    assert_eq!(restored.places, original.places);
    assert_eq!(restored.place_feed, original.place_feed);
    assert_eq!(restored.routes, original.routes);
    assert_eq!(restored.patterns, original.patterns);
    assert_eq!(restored.trips, original.trips);
    assert_eq!(restored.stop_times, original.stop_times);

    // Ensure the rebuilt R-tree spatial index works after rkyv deserialization
    let stop1_coord = restored.places[restored.stop_id_to_place[&StopId::new("stop1")].index()]
        .representative_coord()
        .unwrap();
    let nearby = restored.places_near(stop1_coord, 50.0);
    assert!(!nearby.is_empty());
}
