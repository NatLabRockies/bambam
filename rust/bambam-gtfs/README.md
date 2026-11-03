# bambam-gtfs

GTFS Schedule and GTFS-Flex data structures, phased validation pipeline, and RAPTOR-style routing timetable for BAMBAM.

## Phased Architecture

1. **Phase 1 (`raw::RawFeed` / `raw::GtfsArchive`)**:
   - 1:1 syntactic deserialization of CSV and GeoJSON (`locations.geojson`) files from directories or `.zip` archives.
   - Composable `GtfsTable` and `TableBundle` traits for selective table loading and $O(1)$-memory streaming via `CsvRowIter`.
2. **Phase 2 (`model::ValidatedFeed`)**:
   - Relational foreign-key validation, GTFS-Flex shared namespace collision checks, sum-type normalization (`StopPlace`, `StopTimeTiming`, `BookingNotice`), unified `ServiceCalendar` (`calendar.txt` + `calendar_dates.txt`), parsed `TimeZone` on `Agency`, and `ShapePolyline` Linear Referencing System (`ShapeLinearRef`).
   - Configurable `ValidationPolicy` (`Strict` vs. `DropInvalidTripsWithWarning`) for real-world "dirty GTFS" feeds, recording `ValidationWarning`s and `DroppedEntityCounts`.
3. **Phase 3 (`routing::RoutingTimetable` & `routing::RoutingTimetableBuilder`)**:
   - Contiguous `0..N` typed indices (`FeedIdx`, `PlaceIdx`, `RouteIdx`, `PatternIdx`, `TripIdx`, `ServiceIdx`, `ShapeIdx`), 16-byte `CompactStopTime` CSR arena, intermediate stop-time interpolation, and multi-feed merging with $O(1)$ bidirectional lineage resolution.
   - Unified departure queries via `earliest_boardable_trip` and `earliest_boardable_trip_with_overnight` (supporting fixed schedules via `partition_point`, `frequencies.txt` headways, and GTFS-Flex windows, plus optional frequency trip materialization).
   - Exact ray-casting point-in-polygon (`contains_coord`) on `FlexLocation` and an `rstar::RTree` spatial index (`places_near`, `places_in_bbox`) for $O(\log N)$ multimodal street-network linking.
   - Timezone and overnight service-day normalization (`active_services_at_time`, `active_services_for_feed_at_timestamp`).
   - Instant HPC cold-start binary snapshot serialization via `bincode` (`to_bincode_bytes`, `from_bincode_bytes`, `save_bincode`, `load_bincode`).

