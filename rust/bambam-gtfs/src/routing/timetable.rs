//! Phase 3 RAPTOR-style contiguous CSR timetable (`RaptorTimetable`), spatial/departure query methods,
//! and `rkyv` binary snapshot serialization.

use crate::ids::*;
use crate::model::{DateMappingPolicy, Route, ServiceCalendar, ShapePolyline, StopPlace};
use crate::primitives::{BoundingBox, Coord, GtfsTime};
#[cfg(feature = "gtfs-flex")]
use crate::routing::compact_stop_time::NO_FLEX_DURATION_IDX;
use crate::routing::lineage::resolve_route_agency;
use crate::routing::spatial_index::{
    deserialize_spatial_index, serialize_spatial_index, RkyvSpatialIndex,
};
use crate::routing::{
    ActiveServiceWindow, BoardableDeparture, CompactStopTime, FeedMetadata, FullyQualifiedPlaceId,
    FullyQualifiedRouteId, FullyQualifiedTripId, IndexedTransfer, PlaceIdKind,
    PlacePatternOccurrence, PlaceSpatialEntry, RaptorTimetableBuilder, RoutableTrip, RoutePattern,
    RoutingError,
};
use crate::traits::Spatial;
use chrono::{DateTime, Days, NaiveDate, NaiveDateTime};
use chrono_tz::Tz;
use rstar::{RTree, AABB};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::Arc;

#[cfg(feature = "gtfs-flex")]
use crate::model::FlexDurationParams;
#[cfg(feature = "gtfs-flex")]
use crate::routing::spatial_index::flex_zone_within_radius;

/// Phase 3: RAPTOR-style contiguous CSR timetable, multi-feed archive registry, and spatial/shape
/// index ready for continental-scale vectorized multimodal graph search.
#[derive(
    Clone, Debug, Default, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct RaptorTimetable {
    /// Metadata and index ranges for every merged GTFS archive, indexed by [`FeedIdx`].
    pub feeds: Vec<FeedMetadata>,

    /// Dense table of all routable places (`Stop`, `FlexZone`, `LocationGroup`), indexed by [`PlaceIdx`].
    pub places: Vec<StopPlace>,
    /// For each [`PlaceIdx`], the owning [`FeedIdx`].
    pub place_feed: Vec<FeedIdx>,

    /// R*-tree spatial index over [`PlaceSpatialEntry`] for $O(\log N)$ bounding-box and radius queries.
    #[serde(
        serialize_with = "serialize_spatial_index",
        deserialize_with = "deserialize_spatial_index",
        default
    )]
    #[rkyv(with = RkyvSpatialIndex)]
    pub place_spatial_index: RTree<PlaceSpatialEntry>,

    /// Dense table of all routes, indexed by [`RouteIdx`].
    pub routes: Vec<Route>,
    /// For each [`RouteIdx`], the owning [`FeedIdx`].
    pub route_feed: Vec<FeedIdx>,

    /// Dense table of all unique route patterns, indexed by [`PatternIdx`].
    pub patterns: Vec<RoutePattern>,
    /// Dense table of all trips, indexed by [`TripIdx`].
    pub trips: Vec<RoutableTrip>,

    /// Flat CSR arena of all `16-byte` [`CompactStopTime`] events across all trips.
    ///
    /// Use [`RaptorTimetable::trip_stop_times`] or `timetable.stop_times[trip.stop_times_range()]`
    /// for $O(1)$ slice access without pointer chasing.
    pub stop_times: Vec<CompactStopTime>,

    /// Interned deduplicated table of [`FlexDurationParams`] referenced by `CompactStopTime::flex_duration_idx`.
    #[cfg(feature = "gtfs-flex")]
    pub flex_duration_table: Vec<FlexDurationParams>,

    /// Dense table of all service calendars, indexed by [`ServiceIdx`].
    pub services: Vec<ServiceCalendar>,
    /// For each [`ServiceIdx`], the owning [`FeedIdx`].
    pub service_feed: Vec<FeedIdx>,

    /// Dense table of all shape polylines, indexed by [`ShapeIdx`].
    pub shapes: Vec<Arc<ShapePolyline>>,
    /// For each [`ShapeIdx`], the owning [`FeedIdx`].
    pub shape_feed: Vec<FeedIdx>,

    // --- Reverse Adjacency Indices (indexed by contiguousIdx.index()) ---
    /// For each [`RouteIdx`], the list of [`PatternIdx`]s serving that route.
    pub route_to_patterns: Vec<Vec<PatternIdx>>,
    /// For each [`PlaceIdx`], all `(PatternIdx, stop_offset_in_pattern)` occurrences visiting that place.
    pub place_to_patterns: Vec<Vec<PlacePatternOccurrence>>,
    /// For each [`PlaceIdx`], outgoing transfer and pathway edges to other [`PlaceIdx`]s.
    pub place_to_transfers: Vec<Vec<IndexedTransfer>>,
    /// For each [`PlaceIdx`] that is a `LocationGroup`, the [`PlaceIdx`]s of its constituent member stops/zones.
    pub group_to_member_places: Vec<Vec<PlaceIdx>>,

    // --- Scoped (Multi-Feed Safe) Lookup Maps ---
    /// Lookup from [`FeedId`] to [`FeedIdx`].
    pub feed_id_to_idx: HashMap<FeedId, FeedIdx>,
    /// Scoped lookup from `(FeedIdx, StopId)` to [`PlaceIdx`].
    pub scoped_stop_to_place: HashMap<(FeedIdx, StopId), PlaceIdx>,
    /// Scoped lookup from `(FeedIdx, LocationId)` to [`PlaceIdx`] (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub scoped_location_to_place: HashMap<(FeedIdx, LocationId), PlaceIdx>,
    /// Scoped lookup from `(FeedIdx, LocationGroupId)` to [`PlaceIdx`] (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub scoped_location_group_to_place: HashMap<(FeedIdx, LocationGroupId), PlaceIdx>,
    /// Scoped lookup from `(FeedIdx, RouteId)` to [`RouteIdx`].
    pub scoped_route_to_idx: HashMap<(FeedIdx, RouteId), RouteIdx>,
    /// Scoped lookup from `(FeedIdx, TripId)` to [`TripIdx`].
    pub scoped_trip_to_idx: HashMap<(FeedIdx, TripId), TripIdx>,
    /// Scoped lookup from `(FeedIdx, ServiceId)` to [`ServiceIdx`].
    pub scoped_service_to_idx: HashMap<(FeedIdx, ServiceId), ServiceIdx>,
    /// Scoped lookup from `(FeedIdx, ShapeId)` to [`ShapeIdx`].
    pub scoped_shape_to_idx: HashMap<(FeedIdx, ShapeId), ShapeIdx>,

    // --- Convenience Single-Feed / Last-Write Lookup Maps ---
    /// Convenience lookup from `StopId` to [`PlaceIdx`].
    pub stop_id_to_place: HashMap<StopId, PlaceIdx>,
    /// Convenience lookup from `LocationId` to [`PlaceIdx`] (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub location_id_to_place: HashMap<LocationId, PlaceIdx>,
    /// Convenience lookup from `LocationGroupId` to [`PlaceIdx`] (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub location_group_id_to_place: HashMap<LocationGroupId, PlaceIdx>,
    /// Convenience lookup from `RouteId` to [`RouteIdx`].
    pub route_id_to_idx: HashMap<RouteId, RouteIdx>,
    /// Convenience lookup from `TripId` to [`TripIdx`].
    pub trip_id_to_idx: HashMap<TripId, TripIdx>,
    /// Convenience lookup from `ServiceId` to [`ServiceIdx`].
    pub service_id_to_idx: HashMap<ServiceId, ServiceIdx>,
    /// Convenience lookup from `ShapeId` to [`ShapeIdx`].
    pub shape_id_to_idx: HashMap<ShapeId, ShapeIdx>,
}

impl RaptorTimetable {
    /// Creates a new [`RaptorTimetableBuilder`] for merging one or more [`crate::model::ValidatedFeed`]s
    /// into a single contiguous national/regional [`RaptorTimetable`].
    pub fn builder() -> RaptorTimetableBuilder {
        RaptorTimetableBuilder::new()
    }

    /// Returns the contiguous slice of [`CompactStopTime`]s for `trip_idx` in $O(1)$ time.
    #[inline]
    pub fn trip_stop_times(&self, trip_idx: TripIdx) -> &[CompactStopTime] {
        let Some(trip) = self.trips.get(trip_idx.index()) else {
            return &[];
        };
        &self.stop_times[trip.stop_times_range()]
    }

    /// Returns the [`PlaceIdx`] visited by `trip_idx` at `stop_offset` in $O(1)$ time via its [`RoutePattern`].
    #[inline]
    pub fn trip_stop_place(&self, trip_idx: TripIdx, stop_offset: usize) -> Option<PlaceIdx> {
        let trip = self.trips.get(trip_idx.index())?;
        let pattern = self.patterns.get(trip.pattern_idx.index())?;
        pattern.stops.get(stop_offset).map(|s| s.place_idx)
    }

    /// Resolves any interned [`FlexDurationParams`] for a [`CompactStopTime`].
    #[cfg(feature = "gtfs-flex")]
    pub fn stop_time_flex_duration(&self, st: CompactStopTime) -> FlexDurationParams {
        if st.flex_duration_idx == NO_FLEX_DURATION_IDX {
            FlexDurationParams::default()
        } else {
            self.flex_duration_table
                .get(st.flex_duration_idx as usize)
                .copied()
                .unwrap_or_default()
        }
    }

    // --- O(1) Reverse Lookups (Global Index -> Fully-Qualified Lineage & Timezone) ---

    /// Returns the primary IANA [`chrono_tz::Tz`] timezone for `feed_idx` (from `agency.txt`).
    #[inline]
    pub fn feed_timezone(&self, feed_idx: FeedIdx) -> Option<Tz> {
        self.feeds
            .get(feed_idx.index())
            .and_then(|f| f.primary_timezone)
    }

    /// Resolves the IANA [`chrono_tz::Tz`] timezone for `route_idx` in $O(1)$ time
    /// (matching `Route::agency_id` against the feed's agencies, or falling back to the feed's `primary_timezone`).
    pub fn route_timezone(&self, route_idx: RouteIdx) -> Option<Tz> {
        let route = self.routes.get(route_idx.index())?;
        let feed_idx = *self.route_feed.get(route_idx.index())?;
        let feed = self.feeds.get(feed_idx.index())?;
        let (_, _, tz) = resolve_route_agency(route, &feed.agencies);
        tz.or(feed.primary_timezone)
    }

    /// Resolves the IANA [`chrono_tz::Tz`] timezone for `trip_idx` in $O(1)$ time via its route and feed.
    pub fn trip_timezone(&self, trip_idx: TripIdx) -> Option<Tz> {
        let trip = self.trips.get(trip_idx.index())?;
        self.route_timezone(trip.route_idx)
    }

    /// Resolves the local IANA [`chrono_tz::Tz`] timezone for `place_idx` in $O(1)$ time
    /// (preferring `Stop::timezone` if overridden on a stop, otherwise returning the owning feed's `primary_timezone`).
    pub fn place_timezone(&self, place_idx: PlaceIdx) -> Option<Tz> {
        let place = self.places.get(place_idx.index())?;
        if let Some(s) = place.as_stop() {
            if let Some(tz) = s.timezone {
                return Some(tz);
            }
        }
        let feed_idx = *self.place_feed.get(place_idx.index())?;
        self.feed_timezone(feed_idx)
    }

    /// Appends the agency timezone of `trip_idx` to `(service_date, time)` to construct a
    /// timezone-aware [`DateTime<Tz>`].
    pub fn resolve_trip_datetime(
        &self,
        trip_idx: TripIdx,
        service_date: NaiveDate,
        time: GtfsTime,
    ) -> Option<DateTime<Tz>> {
        let tz = self.trip_timezone(trip_idx)?;
        time.to_zoned_datetime(service_date, tz)
    }

    /// Resolves a [`TripIdx`] in $O(1)$ time (three direct slice reads, zero hash lookups) into its
    /// [`FullyQualifiedTripId`]: `(FeedId, AgencyId, RouteId, TripId, Tz)`.
    pub fn resolve_trip_id(&self, trip_idx: TripIdx) -> Option<FullyQualifiedTripId<'_>> {
        let trip = self.trips.get(trip_idx.index())?;
        let route = self.routes.get(trip.route_idx.index())?;
        let feed = self.feeds.get(trip.feed_idx.index())?;

        let (agency_id, agency_name, tz) = resolve_route_agency(route, &feed.agencies);

        Some(FullyQualifiedTripId {
            feed_idx: trip.feed_idx,
            feed_id: &feed.id,
            agency_id,
            agency_name,
            timezone: tz.or(feed.primary_timezone),
            route_id: &route.id,
            trip_id: &trip.id,
        })
    }

    /// Resolves a [`RouteIdx`] in $O(1)$ time into its [`FullyQualifiedRouteId`]: `(FeedId, AgencyId, RouteId, Tz)`.
    pub fn resolve_route_id(&self, route_idx: RouteIdx) -> Option<FullyQualifiedRouteId<'_>> {
        let route = self.routes.get(route_idx.index())?;
        let feed_idx = *self.route_feed.get(route_idx.index())?;
        let feed = self.feeds.get(feed_idx.index())?;
        let (agency_id, _, tz) = resolve_route_agency(route, &feed.agencies);

        Some(FullyQualifiedRouteId {
            feed_idx,
            feed_id: &feed.id,
            agency_id,
            timezone: tz.or(feed.primary_timezone),
            route_id: &route.id,
        })
    }

    /// Resolves a [`PlaceIdx`] in $O(1)$ time into its [`FullyQualifiedPlaceId`]:
    /// `(FeedId, StopId | LocationId | LocationGroupId, Tz)`.
    pub fn resolve_place_id(&self, place_idx: PlaceIdx) -> Option<FullyQualifiedPlaceId<'_>> {
        let place = self.places.get(place_idx.index())?;
        let feed_idx = *self.place_feed.get(place_idx.index())?;
        let feed = self.feeds.get(feed_idx.index())?;

        let (place_id, stop_tz) = match place {
            StopPlace::Stop(s) => (PlaceIdKind::Stop(&s.id), s.timezone),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::FlexZone(z) => (PlaceIdKind::FlexZone(&z.id), None),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::LocationGroup(g) => (PlaceIdKind::LocationGroup(&g.id), None),
        };

        Some(FullyQualifiedPlaceId {
            feed_idx,
            feed_id: &feed.id,
            place_id,
            timezone: stop_tz.or(feed.primary_timezone),
        })
    }

    // --- O(1) Forward Lookups (Fully-Qualified / Scoped ID -> Global Index) ---

    /// Looks up a [`PlaceIdx`] by `(FeedId, StopId)` in $O(1)$ time.
    pub fn lookup_stop(&self, feed_id: &FeedId, stop_id: &StopId) -> Option<PlaceIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_stop_to_place
            .get(&(feed_idx, stop_id.clone()))
            .copied()
    }

    /// Looks up a [`PlaceIdx`] by `(FeedId, LocationId)` in $O(1)$ time (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub fn lookup_flex_location(
        &self,
        feed_id: &FeedId,
        location_id: &LocationId,
    ) -> Option<PlaceIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_location_to_place
            .get(&(feed_idx, location_id.clone()))
            .copied()
    }

    /// Looks up a [`PlaceIdx`] by `(FeedId, LocationGroupId)` in $O(1)$ time (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub fn lookup_location_group(
        &self,
        feed_id: &FeedId,
        group_id: &LocationGroupId,
    ) -> Option<PlaceIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_location_group_to_place
            .get(&(feed_idx, group_id.clone()))
            .copied()
    }

    /// Looks up a [`RouteIdx`] by `(FeedId, RouteId)` in $O(1)$ time.
    pub fn lookup_route(&self, feed_id: &FeedId, route_id: &RouteId) -> Option<RouteIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_route_to_idx
            .get(&(feed_idx, route_id.clone()))
            .copied()
    }

    /// Looks up a [`TripIdx`] by `(FeedId, TripId)` in $O(1)$ time.
    pub fn lookup_trip(&self, feed_id: &FeedId, trip_id: &TripId) -> Option<TripIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_trip_to_idx
            .get(&(feed_idx, trip_id.clone()))
            .copied()
    }

    /// Resolves `target_date` to a representative active service date (`picked_date`) for `feed_idx`
    /// by applying `policy` across that feed's [`ServiceCalendar`] slice (`feed.services_span`).
    pub fn resolve_service_date_for_feed(
        &self,
        feed_idx: FeedIdx,
        target_date: NaiveDate,
        policy: DateMappingPolicy,
    ) -> Option<NaiveDate> {
        let feed = self.feeds.get(feed_idx.index())?;
        let start = feed.services_span.start as usize;
        let end = feed.services_span.end as usize;
        let feed_services = self.services.get(start..end)?;
        ServiceCalendar::resolve_service_date(feed_services, target_date, policy)
    }

    fn populate_feed_active_services(
        &self,
        feed_idx: FeedIdx,
        query_date: NaiveDate,
        policy: DateMappingPolicy,
        today_active: &mut [bool],
        yesterday_active: &mut [bool],
        feed_picked_dates: &mut [Option<NaiveDate>],
    ) -> Option<()> {
        let feed = self.feeds.get(feed_idx.index())?;
        let start = feed.services_span.start as usize;
        let end = feed.services_span.end as usize;
        let feed_services = self.services.get(start..end)?;

        let picked = match policy {
            DateMappingPolicy::Exact => Some(query_date),
            _ => ServiceCalendar::resolve_service_date(feed_services, query_date, policy),
        };

        if let Some(picked_date) = picked {
            feed_picked_dates[feed_idx.index()] = Some(picked_date);
            let yesterday_date = picked_date
                .checked_sub_days(Days::new(1))
                .unwrap_or(picked_date);
            for (offset, service) in feed_services.iter().enumerate() {
                let sidx = start + offset;
                today_active[sidx] = service.is_active_on(picked_date);
                yesterday_active[sidx] = service.is_active_on(yesterday_date);
            }
        }
        Some(())
    }

    /// Evaluates active services across all feeds for a local `(date, time_of_day)` query using
    /// [`DateMappingPolicy::Exact`], including previous-day (`date - 1`) services that may still
    /// be running overnight with GTFS timestamps `>= 24:00:00` (`time_of_day + 86_400` seconds).
    pub fn active_services_at_time(
        &self,
        date: NaiveDate,
        time_of_day: GtfsTime,
    ) -> ActiveServiceWindow {
        self.active_services_at_time_with_policy(date, time_of_day, DateMappingPolicy::Exact)
    }

    /// Evaluates active services for `feed_idx` at a local `(query_date, time_of_day)` using `policy`
    /// to map `query_date` to a representative `picked_date` for that feed.
    ///
    /// Only services belonging to `feed_idx` (`feed.services_span`) are enabled in the returned
    /// [`ActiveServiceWindow`], and `window.date` preserves the rider's `query_date` so boarding
    /// and arrival [`DateTime`]s are resolved on the rider's actual query date.
    ///
    /// Under [`DateMappingPolicy::Exact`], `feed_picked_dates[feed_idx]` is `Some(query_date)` and
    /// both `query_date` (`today_active`) and `query_date - 1` (`yesterday_active`) are evaluated
    /// directly even when no service starts on `query_date`, preserving overnight (`>= 24:00:00`)
    /// departures from the previous calendar day. For non-exact policies, if no date satisfies
    /// `policy` for `feed_idx`, `feed_picked_dates[feed_idx]` is `None` and `today_active` and
    /// `yesterday_active` will be all `false`.
    pub fn active_services_for_feed_with_policy(
        &self,
        feed_idx: FeedIdx,
        query_date: NaiveDate,
        time_of_day: GtfsTime,
        policy: DateMappingPolicy,
    ) -> Option<ActiveServiceWindow> {
        let n_services = self.services.len();
        let mut today_active = vec![false; n_services];
        let mut yesterday_active = vec![false; n_services];
        let mut feed_picked_dates = vec![None; self.feeds.len()];

        self.populate_feed_active_services(
            feed_idx,
            query_date,
            policy,
            &mut today_active,
            &mut yesterday_active,
            &mut feed_picked_dates,
        )?;

        Some(ActiveServiceWindow {
            date: query_date,
            feed_picked_dates,
            time_of_day,
            today_active,
            yesterday_overnight_time: time_of_day.add_seconds(86_400),
            yesterday_active,
        })
    }

    /// Evaluates active services across **all** feeds in this [`RaptorTimetable`] for a local
    /// `(query_date, time_of_day)` by resolving each feed's representative date independently
    /// using `policy` and recording it in [`ActiveServiceWindow::feed_picked_dates`].
    ///
    /// Under [`DateMappingPolicy::Exact`], both `query_date` and `query_date - 1` are evaluated
    /// directly for every feed even when no service starts on `query_date`. For non-exact policies,
    /// feeds with no matching active date within tolerance have `feed_picked_dates[feed_idx] == None`
    /// and both masks left `false`.
    pub fn active_services_at_time_with_policy(
        &self,
        query_date: NaiveDate,
        time_of_day: GtfsTime,
        policy: DateMappingPolicy,
    ) -> ActiveServiceWindow {
        let n_services = self.services.len();
        let mut today_active = vec![false; n_services];
        let mut yesterday_active = vec![false; n_services];
        let mut feed_picked_dates = vec![None; self.feeds.len()];

        for i in 0..self.feeds.len() {
            let _ = self.populate_feed_active_services(
                FeedIdx::from_usize(i),
                query_date,
                policy,
                &mut today_active,
                &mut yesterday_active,
                &mut feed_picked_dates,
            );
        }

        ActiveServiceWindow {
            date: query_date,
            feed_picked_dates,
            time_of_day,
            today_active,
            yesterday_overnight_time: time_of_day.add_seconds(86_400),
            yesterday_active,
        }
    }

    /// Evaluates active services for `feed_idx` at a timezone-aware [`DateTime`] by converting
    /// the instant into the feed's primary IANA [`chrono_tz::Tz`] timezone (from `agency.txt`)
    /// using [`DateMappingPolicy::Exact`].
    pub fn active_services_for_feed_at_datetime<T: chrono::TimeZone>(
        &self,
        feed_idx: FeedIdx,
        dt: &DateTime<T>,
    ) -> Option<ActiveServiceWindow> {
        self.active_services_for_feed_at_datetime_with_policy(
            feed_idx,
            dt,
            DateMappingPolicy::Exact,
        )
    }

    /// Evaluates active services for `feed_idx` at a timezone-aware [`DateTime`] by converting
    /// the instant into the feed's primary IANA [`chrono_tz::Tz`] timezone and resolving the feed's
    /// active service date via `policy`.
    pub fn active_services_for_feed_at_datetime_with_policy<T: chrono::TimeZone>(
        &self,
        feed_idx: FeedIdx,
        dt: &DateTime<T>,
        policy: DateMappingPolicy,
    ) -> Option<ActiveServiceWindow> {
        let tz = self.feed_timezone(feed_idx)?;
        let local_dt = dt.with_timezone(&tz);
        let (date, time_of_day) = GtfsTime::from_datetime(&local_dt);
        self.active_services_for_feed_with_policy(feed_idx, date, time_of_day, policy)
    }

    /// Finds the earliest boardable trip on `pattern_idx` at `stop_offset` at or after a timezone-aware
    /// [`DateTime`] across a merged multi-timezone [`RaptorTimetable`] using [`DateMappingPolicy::Exact`].
    ///
    /// Automatically converts `dt` into the pattern's owning feed's local [`chrono_tz::Tz`] timezone,
    /// evaluates both current-day and previous-day overnight services in that feed's local service day,
    /// and returns both the [`BoardableDeparture`] and the exact [`DateTime<Tz>`] of boarding.
    pub fn earliest_boardable_trip_at_datetime<T: chrono::TimeZone>(
        &self,
        pattern_idx: PatternIdx,
        stop_offset: usize,
        dt: &DateTime<T>,
    ) -> Option<(BoardableDeparture, DateTime<Tz>)> {
        self.earliest_boardable_trip_at_datetime_with_policy(
            pattern_idx,
            stop_offset,
            dt,
            DateMappingPolicy::Exact,
        )
    }

    /// Finds the earliest boardable trip on `pattern_idx` at `stop_offset` at or after a timezone-aware
    /// [`DateTime`] across a merged multi-timezone [`RaptorTimetable`], resolving the owning feed's
    /// active service date using `policy`.
    pub fn earliest_boardable_trip_at_datetime_with_policy<T: chrono::TimeZone>(
        &self,
        pattern_idx: PatternIdx,
        stop_offset: usize,
        dt: &DateTime<T>,
        policy: DateMappingPolicy,
    ) -> Option<(BoardableDeparture, DateTime<Tz>)> {
        let pattern = self.patterns.get(pattern_idx.index())?;
        let window =
            self.active_services_for_feed_at_datetime_with_policy(pattern.feed_idx, dt, policy)?;
        let dep = self.earliest_boardable_trip(pattern_idx, stop_offset, &window)?;
        let dep_dt = dep.departure_datetime(self, window.date)?;
        Some((dep, dep_dt))
    }

    /// Infers the local IANA [`chrono_tz::Tz`] timezone for a query at `coord` by checking transit places
    /// within `radius_meters`, or falling back to the nearest transit place in the R*-tree spatial index.
    ///
    /// Used for nationwide queries that provide a naive local start time (`NaiveDateTime`) without an
    /// explicit timezone, allowing the query to assume the local timezone of nearby transit services.
    pub fn infer_local_timezone(&self, coord: Coord, radius_meters: f64) -> Option<Tz> {
        for pidx in self.places_near(coord, radius_meters) {
            if let Some(tz) = self.place_timezone(pidx) {
                return Some(tz);
            }
        }
        self.nearest_place_timezone(coord)
    }

    /// Returns the local IANA [`chrono_tz::Tz`] timezone of the closest transit place to `coord`
    /// in the [`rstar::RTree`] spatial index in $O(\log N)$ time.
    pub fn nearest_place_timezone(&self, coord: Coord) -> Option<Tz> {
        if let Some(nearest) = self
            .place_spatial_index
            .nearest_neighbor(&[coord.lon, coord.lat])
        {
            if let Some(tz) = self.place_timezone(nearest.place_idx) {
                return Some(tz);
            }
        }
        self.feeds.iter().find_map(|f| f.primary_timezone)
    }

    /// Resolves a timezone-unaware query `naive_dt` at `coord` (within `radius_meters`) into a
    /// timezone-aware [`DateTime<Tz>`] by assuming the local [`chrono_tz::Tz`] timezone of the
    /// transit services found at or nearest to `coord`.
    pub fn localize_naive_query_datetime(
        &self,
        naive_dt: NaiveDateTime,
        coord: Coord,
        radius_meters: f64,
    ) -> Option<DateTime<Tz>> {
        let tz = self.infer_local_timezone(coord, radius_meters)?;
        naive_dt.and_local_timezone(tz).earliest()
    }

    /// Finds the earliest boardable trip on `pattern_idx` at `stop_offset` departing at or after `after_time`
    /// within a single service-day boolean mask `active_services` (indexed by [`ServiceIdx`]).
    ///
    /// For full service-window queries that also evaluate previous-day overnight trips (`>= 24:00:00`),
    /// use [`RaptorTimetable::earliest_boardable_trip`].
    ///
    /// Seamlessly handles:
    /// 1. **Fixed-schedule trips** (and materialized frequency trips) via `partition_point` binary search
    ///    when the pattern consists purely of non-frequency fixed-schedule trips,
    /// 2. **Headway-based `frequencies.txt` trips** by computing the first headway departure `>= after_time`
    ///    within each `FrequencyWindow`, and
    /// 3. **GTFS-Flex `FlexWindow` trips** where `after_time <= window.end` (boarding at `max(after_time, window.start)`).
    pub fn earliest_boardable_trip_in_mask(
        &self,
        pattern_idx: PatternIdx,
        stop_offset: usize,
        after_time: GtfsTime,
        active_services: &[bool],
    ) -> Option<BoardableDeparture> {
        let pattern = self.patterns.get(pattern_idx.index())?;
        let stop_prof = pattern.stops.get(stop_offset)?;
        if !stop_prof.pickup.is_allowed() || pattern.trips.is_empty() {
            return None;
        }

        let has_freq_or_flex = pattern.trips.iter().any(|&tidx| {
            let t = &self.trips[tidx.index()];
            !t.frequencies.is_empty()
                || self.stop_times[t.stop_times_offset as usize + stop_offset].is_flex_window()
        });

        if !has_freq_or_flex {
            let start_pos = pattern.trips.partition_point(|&tidx| {
                let t = &self.trips[tidx.index()];
                let st = self.stop_times[t.stop_times_offset as usize + stop_offset];
                st.departure_or_end_secs < after_time.0
            });

            for &tidx in &pattern.trips[start_pos..] {
                let t = &self.trips[tidx.index()];
                if !active_services
                    .get(t.service_idx.index())
                    .copied()
                    .unwrap_or(false)
                {
                    continue;
                }
                let st = self.stop_times[t.stop_times_offset as usize + stop_offset];
                if st.departure_or_end_secs >= after_time.0 {
                    return Some(BoardableDeparture {
                        trip_idx: tidx,
                        departure_time: GtfsTime(st.departure_or_end_secs),
                        frequency_offset_secs: 0,
                        is_overnight_from_yesterday: false,
                    });
                }
            }
            return None;
        }

        let mut best: Option<BoardableDeparture> = None;

        for &tidx in &pattern.trips {
            let trip = &self.trips[tidx.index()];
            if !active_services
                .get(trip.service_idx.index())
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let st_slice = &self.stop_times[trip.stop_times_range()];
            let st = st_slice[stop_offset];

            if trip.frequencies.is_empty() {
                let candidate = if st.is_flex_window() {
                    if after_time.0 <= st.departure_or_end_secs {
                        Some(BoardableDeparture {
                            trip_idx: tidx,
                            departure_time: GtfsTime(after_time.0.max(st.arrival_or_start_secs)),
                            frequency_offset_secs: 0,
                            is_overnight_from_yesterday: false,
                        })
                    } else {
                        None
                    }
                } else if st.departure_or_end_secs >= after_time.0 {
                    Some(BoardableDeparture {
                        trip_idx: tidx,
                        departure_time: GtfsTime(st.departure_or_end_secs),
                        frequency_offset_secs: 0,
                        is_overnight_from_yesterday: false,
                    })
                } else {
                    None
                };

                if let Some(cand) = candidate {
                    if best
                        .as_ref()
                        .is_none_or(|b| cand.departure_time < b.departure_time)
                    {
                        best = Some(cand);
                    }
                }
            } else {
                let t0_secs = st_slice[0].departure_or_end_secs as i64;
                let ts_secs = st.departure_or_end_secs as i64;
                let travel_offset_from_start = ts_secs - t0_secs;

                for freq in &trip.frequencies {
                    if freq.headway_secs == 0 {
                        continue;
                    }
                    let win_start = freq.window.start.0 as i64;
                    let win_end = freq.window.end.0 as i64;
                    let headway = freq.headway_secs as i64;

                    let first_dep_at_stop = win_start + travel_offset_from_start;
                    let target = after_time.0 as i64;

                    let k = if target <= first_dep_at_stop {
                        0
                    } else {
                        (target - first_dep_at_stop + headway - 1) / headway
                    };

                    let vehicle_start_at_stop0 = win_start + k * headway;
                    if vehicle_start_at_stop0 < win_end
                        || (k == 0 && win_start == win_end && vehicle_start_at_stop0 == win_end)
                    {
                        let actual_dep_at_stop = vehicle_start_at_stop0 + travel_offset_from_start;
                        if actual_dep_at_stop >= 0 {
                            let dep_time = GtfsTime(actual_dep_at_stop as u32);
                            let freq_offset = vehicle_start_at_stop0 - t0_secs;
                            let cand = BoardableDeparture {
                                trip_idx: tidx,
                                departure_time: dep_time,
                                frequency_offset_secs: freq_offset,
                                is_overnight_from_yesterday: false,
                            };
                            if best
                                .as_ref()
                                .is_none_or(|b| cand.departure_time < b.departure_time)
                            {
                                best = Some(cand);
                            }
                        }
                    }
                }
            }
        }

        best
    }

    /// Finds the earliest boardable trip on `pattern_idx` at `stop_offset` using an [`ActiveServiceWindow`],
    /// checking both today's schedule (`after_time = window.time_of_day`) and yesterday's overnight
    /// trips (`after_time = window.time_of_day + 86_400`) and returning whichever boards earlier.
    pub fn earliest_boardable_trip(
        &self,
        pattern_idx: PatternIdx,
        stop_offset: usize,
        window: &ActiveServiceWindow,
    ) -> Option<BoardableDeparture> {
        let today_cand = self.earliest_boardable_trip_in_mask(
            pattern_idx,
            stop_offset,
            window.time_of_day,
            &window.today_active,
        );
        let overnight_cand = self
            .earliest_boardable_trip_in_mask(
                pattern_idx,
                stop_offset,
                window.yesterday_overnight_time,
                &window.yesterday_active,
            )
            .map(|mut c| {
                c.is_overnight_from_yesterday = true;
                c
            });

        match (today_cand, overnight_cand) {
            (Some(t), Some(o)) => {
                if o.normalized_query_day_departure() <= t.normalized_query_day_departure() {
                    Some(o)
                } else {
                    Some(t)
                }
            }
            (Some(t), None) => Some(t),
            (None, Some(o)) => Some(o),
            (None, None) => None,
        }
    }

    /// Returns all [`PlacePatternOccurrence`]s visiting `place_idx` (including any `LocationGroup`
    /// patterns that contain `place_idx` as a member).
    pub fn patterns_serving_place(&self, place_idx: PlaceIdx) -> &[PlacePatternOccurrence] {
        self.place_to_patterns
            .get(place_idx.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Finds all [`PlaceIdx`]s whose bounding box intersects `bbox` in $O(\log N + K)$ time
    /// using the [`rstar::RTree`] spatial index.
    pub fn places_in_bbox(&self, bbox: BoundingBox) -> Vec<PlaceIdx> {
        if self.place_spatial_index.size() > 0 {
            let envelope =
                AABB::from_corners([bbox.min_lon, bbox.min_lat], [bbox.max_lon, bbox.max_lat]);
            let mut results: Vec<PlaceIdx> = self
                .place_spatial_index
                .locate_in_envelope_intersecting(&envelope)
                .map(|e| e.place_idx)
                .collect();
            results.sort_unstable();
            results
        } else {
            self.places
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    p.bounding_box()
                        .filter(|b| b.intersects(&bbox))
                        .map(|_| PlaceIdx::from_usize(i))
                })
                .collect()
        }
    }

    /// Finds all [`PlaceIdx`]s whose geometry contains `coord` (for `FlexZone` / `LocationGroup`
    /// via exact ray-casting when `radius_meters == 0.0`, or within `radius_meters` of `coord`).
    ///
    /// Uses the [`rstar::RTree`] spatial index for $O(\log N)$ broad-phase filtering followed by:
    /// - exact Haversine distance check (`<= radius_meters`) for fixed `Stop`s, and
    /// - exact point-in-polygon (`contains_coord`) or polygon boundary proximity for `FlexZone`s and `LocationGroup`s.
    pub fn places_near(&self, coord: Coord, radius_meters: f64) -> Vec<PlaceIdx> {
        let lat_deg = radius_meters.max(0.0) / 111_111.0;
        let lon_deg = lat_deg / coord.lat.to_radians().cos().abs().max(1e-4);
        let query_bbox = BoundingBox {
            min_lon: coord.lon - lon_deg,
            min_lat: coord.lat - lat_deg,
            max_lon: coord.lon + lon_deg,
            max_lat: coord.lat + lat_deg,
        };

        let candidates = self.places_in_bbox(query_bbox);
        let mut matches = Vec::new();
        for pidx in candidates {
            let Some(place) = self.places.get(pidx.index()) else {
                continue;
            };
            match place {
                StopPlace::Stop(s) => {
                    if let Some(sc) = s.representative_coord() {
                        if sc.haversine_meters(coord) <= radius_meters {
                            matches.push(pidx);
                        }
                    }
                }
                #[cfg(feature = "gtfs-flex")]
                StopPlace::FlexZone(z) => {
                    if z.contains_coord(coord)
                        || (radius_meters > 0.0 && flex_zone_within_radius(z, coord, radius_meters))
                    {
                        matches.push(pidx);
                    }
                }
                #[cfg(feature = "gtfs-flex")]
                StopPlace::LocationGroup(g) => {
                    if g.contains_coord(coord)
                        || (radius_meters > 0.0
                            && g.members.iter().any(|m| match m {
                                crate::model::PlaceMember::Stop(s) => s
                                    .representative_coord()
                                    .is_some_and(|sc| sc.haversine_meters(coord) <= radius_meters),
                                crate::model::PlaceMember::FlexZone(z) => {
                                    flex_zone_within_radius(z, coord, radius_meters)
                                }
                            }))
                    {
                        matches.push(pidx);
                    }
                }
            }
        }
        matches
    }

    // --- Binary Snapshot Serialization (`rkyv`) for Instant HPC Cold-Starts ---

    /// Serializes this [`RaptorTimetable`] into a compact `rkyv` byte vector.
    pub fn to_rkyv_bytes(&self) -> Result<Vec<u8>, RoutingError> {
        let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        Ok(aligned.into_vec())
    }

    /// Validates and returns a zero-copy [`ArchivedRaptorTimetable`] reference directly from a
    /// 16-byte-aligned `rkyv` byte slice (such as an [`rkyv::util::AlignedVec`] or memory-mapped buffer)
    /// without allocating or copying the timetable's vectors.
    pub fn access_rkyv_bytes(bytes: &[u8]) -> Result<&ArchivedRaptorTimetable, RoutingError> {
        rkyv::access::<ArchivedRaptorTimetable, rkyv::rancor::Error>(bytes)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Deserializes a [`RaptorTimetable`] from an `rkyv` byte slice (validating the archive via `bytecheck`).
    pub fn from_rkyv_bytes(bytes: &[u8]) -> Result<Self, RoutingError> {
        let mut aligned = rkyv::util::AlignedVec::<16>::with_capacity(bytes.len());
        aligned.extend_from_slice(bytes);
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(&aligned)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Writes this [`RaptorTimetable`] as a binary `rkyv` snapshot to `writer`.
    pub fn write_rkyv<W: Write>(&self, mut writer: W) -> Result<(), RoutingError> {
        let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        writer
            .write_all(&aligned)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Reads a [`RaptorTimetable`] from a binary `rkyv` snapshot `reader`.
    pub fn read_rkyv<R: Read>(mut reader: R) -> Result<Self, RoutingError> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        Self::from_rkyv_bytes(&bytes)
    }

    /// Saves this [`RaptorTimetable`] to a file on disk in `rkyv` format.
    pub fn save_rkyv(&self, path: impl AsRef<Path>) -> Result<(), RoutingError> {
        let file = File::create(path).map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        let writer = BufWriter::new(file);
        self.write_rkyv(writer)
    }

    /// Loads a [`RaptorTimetable`] from an `rkyv` snapshot file on disk.
    pub fn load_rkyv(path: impl AsRef<Path>) -> Result<Self, RoutingError> {
        let file = File::open(path).map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        let reader = BufReader::new(file);
        Self::read_rkyv(reader)
    }

    /// Reconstructs the geographic linestring (`Vec<Coord>`) for a transit traversal along `trip_idx`
    /// from `from_stop_offset` to `to_stop_offset` (inclusive 0-based indices into the trip's stops).
    pub fn reconstruct_leg_linestring(
        &self,
        trip_idx: TripIdx,
        from_stop_offset: usize,
        to_stop_offset: usize,
    ) -> Result<Vec<Coord>, RoutingError> {
        let trip =
            self.trips
                .get(trip_idx.index())
                .ok_or(RoutingError::InvalidStopOffsetRange {
                    trip_idx,
                    from_offset: from_stop_offset,
                    to_offset: to_stop_offset,
                    num_stops: 0,
                })?;

        let num_stops = trip.stop_count as usize;
        if from_stop_offset >= num_stops
            || to_stop_offset >= num_stops
            || from_stop_offset > to_stop_offset
        {
            return Err(RoutingError::InvalidStopOffsetRange {
                trip_idx,
                from_offset: from_stop_offset,
                to_offset: to_stop_offset,
                num_stops,
            });
        }

        let st_slice = &self.stop_times[trip.stop_times_range()];
        if let Some(shape_idx) = trip.shape_idx {
            if let Some(shape) = self.shapes.get(shape_idx.index()) {
                let s_m = st_slice[from_stop_offset].shape_meters();
                let e_m = st_slice[to_stop_offset].shape_meters();
                if let (Some(sm), Some(em)) = (s_m, e_m) {
                    return Ok(shape.slice_linestring(sm, em));
                }
            }
        }

        // Fallback: straight-line representative coordinates across the pattern's stops
        let pattern = &self.patterns[trip.pattern_idx.index()];
        let coords = pattern.stops[from_stop_offset..=to_stop_offset]
            .iter()
            .filter_map(|prof| {
                self.places
                    .get(prof.place_idx.index())?
                    .representative_coord()
            })
            .collect();
        Ok(coords)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{PickupDropOffType, TimepointType};
    use crate::model::ValidatedFeed;
    use crate::raw::{RawFeed, RawFrequency};
    use std::path::PathBuf;

    fn fixture_path(rel_path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path)
    }

    #[test]
    fn test_frequency_trips_earliest_boardable_and_materialization() {
        let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        raw.frequencies = Some(vec![RawFrequency {
            trip_id: TripId::new("trip1"),
            start_time: GtfsTime::from_hms(6, 0, 0),
            end_time: GtfsTime::from_hms(7, 0, 0),
            headway_secs: 600,
            exact_times: TimepointType::Approximate,
        }]);
        let validated = ValidatedFeed::try_from(raw).unwrap();

        // 1. Non-materialized frequency query via earliest_boardable_trip_in_mask
        let timetable = RaptorTimetable::try_from(validated.clone()).unwrap();
        let active_mask = vec![true; timetable.services.len()];
        let pat_idx = PatternIdx::new(0);

        let dep0 = timetable
            .earliest_boardable_trip_in_mask(pat_idx, 0, GtfsTime::from_hms(6, 7, 0), &active_mask)
            .expect("should find frequency departure at stop 0");
        assert_eq!(dep0.departure_time, GtfsTime::from_hms(6, 10, 0));
        assert_eq!(
            dep0.arrival_time_at(&timetable, 2),
            Some(GtfsTime::from_hms(8, 10, 0))
        );

        let dep1 = timetable
            .earliest_boardable_trip_in_mask(pat_idx, 1, GtfsTime::from_hms(7, 12, 0), &active_mask)
            .expect("should find frequency departure at stop 1");
        assert_eq!(dep1.departure_time, GtfsTime::from_hms(7, 20, 0));
        assert_eq!(
            dep1.arrival_time_at(&timetable, 2),
            Some(GtfsTime::from_hms(8, 20, 0))
        );

        // 2. Materialized frequency trips in RaptorTimetableBuilder
        let mut builder = RaptorTimetable::builder().with_materialize_frequencies(true);
        builder.add_feed("basic_mat", validated).unwrap();
        let mat_timetable = builder.build();

        assert_eq!(mat_timetable.trips.len(), 6);
        assert!(mat_timetable.trips.iter().all(|t| t.frequencies.is_empty()));

        let mat_active = vec![true; mat_timetable.services.len()];
        let mat_dep0 = mat_timetable
            .earliest_boardable_trip_in_mask(pat_idx, 0, GtfsTime::from_hms(6, 7, 0), &mat_active)
            .expect("should binary-search materialized frequency trips");
        assert_eq!(mat_dep0.departure_time, GtfsTime::from_hms(6, 10, 0));
        assert_eq!(mat_dep0.frequency_offset_secs, 0);
        assert_eq!(
            mat_dep0.arrival_time_at(&mat_timetable, 2),
            Some(GtfsTime::from_hms(8, 10, 0))
        );
    }

    #[test]
    fn test_earliest_boardable_pickup_filtering_service_mask_and_leg_linestring() {
        let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        raw.frequencies = None;
        // Forbid pickup at stop_offset = 1
        raw.stop_times[1].pickup_type = PickupDropOffType::NotAvailable;
        let validated = ValidatedFeed::try_from(raw).unwrap();
        let timetable = RaptorTimetable::try_from(validated).unwrap();

        let active_all = vec![true; timetable.services.len()];
        let inactive_all = vec![false; timetable.services.len()];

        // Pickup forbidden at stop_offset = 1 -> None
        assert!(timetable
            .earliest_boardable_trip_in_mask(
                PatternIdx::new(0),
                1,
                GtfsTime::from_hms(0, 0, 0),
                &active_all
            )
            .is_none());

        // Service inactive -> None
        assert!(timetable
            .earliest_boardable_trip_in_mask(
                PatternIdx::new(0),
                0,
                GtfsTime::from_hms(0, 0, 0),
                &inactive_all
            )
            .is_none());

        // Fallback straight-line leg linestring when shape_idx is None
        let coords = timetable
            .reconstruct_leg_linestring(TripIdx::new(0), 0, 2)
            .expect("straight-line fallback should succeed");
        assert_eq!(coords.len(), 3);

        // Out-of-bounds or inverted stop offsets return InvalidStopOffsetRange
        assert!(matches!(
            timetable.reconstruct_leg_linestring(TripIdx::new(0), 2, 1),
            Err(RoutingError::InvalidStopOffsetRange { .. })
        ));
        assert!(matches!(
            timetable.reconstruct_leg_linestring(TripIdx::new(0), 0, 99),
            Err(RoutingError::InvalidStopOffsetRange { .. })
        ));
    }

    #[test]
    fn test_multi_feed_timezone_retention_and_local_timezone_inference() {
        let feed_paris = ValidatedFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        let mut raw_denver = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        for ag in &mut raw_denver.agencies {
            ag.timezone = chrono_tz::Tz::America__Denver;
        }
        for st in &mut raw_denver.stops {
            if st.latitude.is_some() {
                st.latitude = Some(39.7392);
                st.longitude = Some(-104.9903);
            }
        }
        let feed_denver = ValidatedFeed::try_from(raw_denver).unwrap();

        let mut builder = RaptorTimetable::builder();
        let paris_idx = builder.add_feed("paris", feed_paris).unwrap();
        let denver_idx = builder.add_feed("denver", feed_denver).unwrap();
        let timetable = builder.build();

        assert_eq!(
            timetable.feed_timezone(paris_idx),
            Some(chrono_tz::Tz::Europe__Paris)
        );
        assert_eq!(
            timetable.feed_timezone(denver_idx),
            Some(chrono_tz::Tz::America__Denver)
        );

        let naive_query = NaiveDate::from_ymd_opt(2017, 1, 7)
            .unwrap()
            .and_hms_opt(8, 30, 0)
            .unwrap();
        let paris_coord = Coord::new(2.449386, 48.796058);
        let denver_coord = Coord::new(-104.9903, 39.7392);

        assert_eq!(
            timetable.infer_local_timezone(paris_coord, 500.0),
            Some(chrono_tz::Tz::Europe__Paris)
        );
        assert_eq!(
            timetable.infer_local_timezone(denver_coord, 500.0),
            Some(chrono_tz::Tz::America__Denver)
        );

        let localized_denver = timetable
            .localize_naive_query_datetime(naive_query, denver_coord, 500.0)
            .expect("should localize naive query to America/Denver");
        assert_eq!(localized_denver.timezone(), chrono_tz::Tz::America__Denver);
        assert_eq!(localized_denver.naive_local(), naive_query);
    }

    #[test]
    fn test_rkyv_snapshot_roundtrip() {
        let feed = ValidatedFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        let original = RaptorTimetable::try_from(feed).unwrap();

        let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(&original)
            .expect("should serialize RaptorTimetable to aligned rkyv buffer");
        let archived = RaptorTimetable::access_rkyv_bytes(&aligned)
            .expect("should zero-copy access ArchivedRaptorTimetable");
        assert_eq!(archived.places.len(), original.places.len());

        let bytes = original
            .to_rkyv_bytes()
            .expect("should serialize RaptorTimetable to rkyv");
        assert!(!bytes.is_empty());

        let restored = RaptorTimetable::from_rkyv_bytes(&bytes)
            .expect("should deserialize RaptorTimetable from rkyv");

        assert_eq!(restored.feeds, original.feeds);
        assert_eq!(restored.places, original.places);
        assert_eq!(restored.place_feed, original.place_feed);
        assert_eq!(restored.routes, original.routes);
        assert_eq!(restored.patterns, original.patterns);
        assert_eq!(restored.trips, original.trips);
        assert_eq!(restored.stop_times, original.stop_times);

        let stop1_coord = restored.places[restored.stop_id_to_place[&StopId::new("stop1")].index()]
            .representative_coord()
            .unwrap();
        let nearby = restored.places_near(stop1_coord, 50.0);
        assert!(!nearby.is_empty());
    }
}
