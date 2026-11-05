# bambam-gtfs

`bambam-gtfs` is a Rust crate for reading, validating, and indexing [GTFS Schedule](https://gtfs.org/schedule/) and [GTFS-Flex](https://gtfs.org/extensions/flex/) transit feeds for multimodal route planning and accessibility modeling in [BAMBAM](https://github.com/NatLabRockies/bambam).

It is designed to scale from inspecting a single transit agency archive up to merging hundreds or thousands of feeds into a single, compact routing timetable with fast spatial queries, linear-referenced shape geometry, and instant binary snapshot loading.

## Alternatives

`bambam-gtfs` owes its beginnings to [`gtfs-structures`](https://crates.io/crates/gtfs-structures), from which this crate borrows several of the core GTFS enum and data structure implementations. `gtfs-structures` is a community-driven project with two active maintainers and clear focus. Consider using that crate if your use case is GTFS analysis and editing. Use `bambam-gtfs` if you need more fine grained control or want to leverage the `RaptorTimetable` or `EdgeTimetable` data models for transit route planning. This project may move slower to update than `gtfs-structures`.

## Features

`bambam-gtfs` was built to add the following features to GTFS reading:
  - strong type system guarantees
  - lazy/granular + streaming IO, allowing users to read only specific GTFS files from an archive
  - merge archives
  - GTFS-Flex compiler feature, data model changes modeled via feature-flagged enum variants
  - ability to construct geometries along shapes between stops (for map plotting a trip/trip segment)
  - phased initialization approach to reading a GTFS collection:
    - a collection of Vectors of data rows in [`bambam_gtfs::raw::RawFeed`]
    - a validated collection of hash maps in [`bambam_gtfs::model::ValidatedFeed`]
    - a vectorized pattern representation suitable for [RAPTOR](https://www.microsoft.com/en-us/research/wp-content/uploads/2012/01/raptor_alenex.pdf) route planning in [`bambam_gtfs::routing::RaptorTimetable`]
    - a flat 3-array CSR edge schedule representation suitable for time-dependent graph search (`routee-compass`) in [`bambam_gtfs::routing::EdgeTimetable`]
  - for `RaptorTimetable` and `EdgeTimetable`, binary load/save to disk backed by [rkyv](https://crates.io/crates/rkyv)

## High-Level Example

```rust
use bambam_gtfs::{
    raw::{RawRoute, RawStopTime},
    Coord, GtfsArchive, GtfsTime, PatternIdx, RaptorTimetable, ValidatedFeed, ValidationPolicy,
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

    // 3. Build a multi-feed RAPTOR-style RaptorTimetable
    let mut builder = RaptorTimetable::builder()
        .with_validation_policy(ValidationPolicy::DropInvalidTripsWithWarning);
    builder.add_feed("agency-a", feed)?;
    let timetable = builder.build();

    // 4. Spatial query: find stops or GTFS-Flex zones within 500 meters of a coordinate
    let _nearby_places = timetable.places_near(Coord::new(-4.486, 48.390), 500.0);

    // 5. Departure query: find the earliest boardable trip on pattern 0 at stop offset 0
    let date = NaiveDate::from_ymd_opt(2017, 1, 7).unwrap();
    let window = timetable.active_services_at_time(date, GtfsTime::from_hms(8, 30, 0));
    if let Some(dep) = timetable.earliest_boardable_trip(PatternIdx::new(0), 0, &window) {
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
    let restored = RaptorTimetable::from_rkyv_bytes(&bytes)?;
    assert_eq!(restored.trips.len(), timetable.trips.len());

    Ok(())
}
```

## License

BSD-Clause 3

Copyright 2025 Alliance for Energy Innovation, LLC

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS “AS IS” AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
