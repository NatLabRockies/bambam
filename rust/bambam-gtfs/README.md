# bambam-gtfs

`bambam-gtfs` is a Rust crate for reading, validating, and indexing [GTFS Schedule](https://gtfs.org/schedule/) and [GTFS-Flex](https://gtfs.org/extensions/flex/) transit feeds for multimodal route planning and accessibility modeling in [BAMBAM](https://github.com/NatLabRockies/bambam).

It is designed to scale from inspecting a single transit agency archive up to merging hundreds or thousands of feeds into a single, compact routing timetable with fast spatial queries, linear-referenced shape geometry, and instant binary snapshot loading.

## Attribution

This crate builds upon and draws inspiration from [`gtfs-structures`](https://crates.io/crates/gtfs-structures), which served as the foundation for this work and provided several of the core GTFS enum and data structure implementations.

## How `bambam-gtfs` Differs from `gtfs-structures`

While `gtfs-structures` is a great general-purpose GTFS reader, `bambam-gtfs` re-architects the data model and loading pipeline around the needs of continental-scale multimodal routing and GTFS extensions:

- **Sum Types (`enum`s) for GTFS Extensions**: Rather than scattering mutually exclusive `Option<T>` fields across structs (which can vary depending on whether GTFS-Flex or other extensions are enabled), `bambam-gtfs` normalizes polymorphic concepts into strict enums—such as `StopPlace` (`Stop`, `FlexZone`, or `LocationGroup`), `StopTimeTiming` (`Scheduled`, `Uninterpolated`, or `FlexWindow`), and `BookingNotice`. This preserves type safety and Cargo feature additivity whether `gtfs-flex` is enabled or disabled.
- **Strongly-Typed Identifiers & Dense Indices**: Entity IDs (`StopId`, `RouteId`, `TripId`, `ServiceId`, `ShapeId`, `LocationId`, etc.) and Phase 3 contiguous array indices (`PlaceIdx`, `RouteIdx`, `PatternIdx`, `TripIdx`, `FeedIdx`) are distinct newtypes rather than raw `String`s or `usize`s, preventing accidental mix-ups across tables or across merged feeds.
- **Composable, Non-Monolithic Table Loading**: Instead of requiring callers to load an entire feed into a single monolithic struct, Phase 1 provides `GtfsArchive`, `GtfsTable`, and `TableBundle`. You can selectively load individual tables, tuples of tables (e.g., `(Vec<RawRoute>, Option<Vec<RawCalendar>>)`), extension bundles (`FlexTables`), or stream massive tables like `stop_times.txt` and `shapes.txt` row-by-row in $O(1)$ memory via `CsvRowIter`.
- **Phased Pipeline & RAPTOR-Style Routing Timetable**: Reading is separated into three explicit phases:
  1. **Phase 1 (`raw::RawFeed`)**: 1:1 syntactic CSV/GeoJSON deserialization.
  2. **Phase 2 (`model::ValidatedFeed`)**: Relational foreign-key validation, `ServiceCalendar` unification, `jiff::tz::TimeZone` parsing, and configurable lenience (`ValidationPolicy::Strict` vs. `DropInvalidTripsWithWarning`) for messy real-world feeds.
  3. **Phase 3 (`routing::RoutingTimetable`)**: A RAPTOR-style contiguous CSR timetable that groups trips into `RoutePattern`s, interpolates intermediate stop times into 16-byte `CompactStopTime` records, indexes stops and flex zones in an `rstar::RTree` (with exact point-in-polygon checks), and supports unified departure queries (`earliest_boardable_trip`) across fixed schedules, `frequencies.txt` headways, overnight trips, and GTFS-Flex windows.
- **Linear Referencing System (LRS) for Route Geometry**: `ShapePolyline` projects stops onto `shapes.txt` polylines and stores cumulative arc-length references (`ShapeLinearRef`). During route planning, any transit leg between two stop offsets on a trip can be sliced into an exact sub-segment linestring (`Vec<Coord>`) via `reconstruct_leg_linestring`.
- **Zero-Copy / Fast Binary Snapshots with `rkyv`**: `RoutingTimetable` and all of its constituent types implement `rkyv::{Archive, Serialize, Deserialize}`, allowing a compiled multi-feed timetable to be saved once to disk and loaded (or accessed zero-copy via `access_rkyv_bytes`) in milliseconds on subsequent runs.

## High-Level Example

```rust
use bambam_gtfs::{
     raw::{RawRoute, RawStopTime},
    Coord, GtfsArchive, GtfsTime, PatternIdx, RoutingTimetable, ValidatedFeed, ValidationPolicy,
};
use chrono::NaiveDate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Selective or streaming access to raw tables without loading the whole archive
    let mut archive = GtfsArchive::open("fixtures/zips/gtfs.zip")?;
    let routes: Vec<RawRoute> = archive.load()?;
    let stop_time_count = archive.table_iter::<RawStopTime>()?.count();
    println!("Loaded {} routes and streamed {} stop_times", routes.len(), stop_time_count);

    // 2. Validate a feed (using lenient policy to drop malformed trips with warnings)
    let feed = ValidatedFeed::from_path_with_policy(
        "fixtures/basic",
        ValidationPolicy::DropInvalidTripsWithWarning,
    )?;

    // 3. Build a multi-feed RAPTOR-style RoutingTimetable
    let mut builder = RoutingTimetable::builder()
        .with_validation_policy(ValidationPolicy::DropInvalidTripsWithWarning);
    builder.add_feed("agency-a", feed)?;
    let timetable = builder.build();

    // 4. Spatial query: find stops or GTFS-Flex zones within 500 meters of a coordinate
    let _nearby_places = timetable.places_near(Coord::new(-4.486, 48.390), 500.0);

    // 5. Departure query: find the earliest boardable trip on pattern 0 at stop offset 0
    let date = NaiveDate::from_ymd_opt(2017, 1, 7).unwrap();
    let window = timetable.active_services_at_time(date, GtfsTime::from_hms(8, 30, 0));
    if let Some(dep) =
        timetable.earliest_boardable_trip_with_overnight(PatternIdx::new(0), 0, &window)
    {
        // Reconstruct the exact geographic linestring between stop 0 and stop 1
        let leg_geometry = timetable.reconstruct_leg_linestring(dep.trip_idx, 0, 1)?;
        println!(
            "Board trip {:?} at {} ({} geometry points)",
            timetable.resolve_trip_id(dep.trip_idx),
            dep.departure_time,
            leg_geometry.len()
        );
    }

    // 6. Serialize and reload a binary snapshot via rkyv
    let bytes = timetable.to_rkyv_bytes()?;
    let restored = RoutingTimetable::from_rkyv_bytes(&bytes)?;
    assert_eq!(restored.trips.len(), timetable.trips.len());

    Ok(())
}
```


