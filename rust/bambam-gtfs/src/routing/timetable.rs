//! Phase 3 RAPTOR-style contiguous CSR timetable, multi-feed merger, and O(1) bidirectional
//! fully-qualified identifier registry designed for continental-scale vectorized multimodal A* search.

use crate::enums::TransferType;
use crate::ids::*;
use crate::model::{
    Agency, BoardingPolicy, DroppedEntityCounts, FrequencyWindow, Route, ServiceCalendar,
    ShapePolyline, StopPlace, StopTime, StopTimeTiming, Trip, ValidatedFeed, ValidationPolicy,
    ValidationWarning,
};
use crate::primitives::{BoundingBox, Coord, GtfsTime, TimeWindow};
use crate::routing::RoutingError;
use crate::traits::{Spatial, TemporalStopTime};
use chrono::{Days, NaiveDate};
use jiff::tz::TimeZone;
use rstar::{RTree, RTreeObject, AABB};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

#[cfg(feature = "gtfs-flex")]
use crate::model::FlexDurationParams;

/// Interpolated or flexible timing at a single stop event in the Phase 3 [`RoutingTimetable`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum RoutableStopTiming {
    /// Fixed or interpolated arrival and departure timestamps (`arrival <= departure`).
    Fixed {
        /// Arrival time at the stop.
        arrival: GtfsTime,
        /// Departure time from the stop.
        departure: GtfsTime,
    },
    /// Demand-responsive pickup/drop-off window (`[start, end]`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    FlexWindow(TimeWindow),
}

const FLAG_IS_FLEX_WINDOW: u16 = 1 << 0;
const NO_FLEX_DURATION_IDX: u16 = u16::MAX;

/// Ultra-compact `16-byte` stop-time entry stored in the flat CSR [`RoutingTimetable::stop_times`] arena.
///
/// Why 16 bytes?
/// - Across 1,000 US GTFS feeds (~200 million stop_times), a 16-byte record consumes only **3.2 GB**
///   total (compared to ~24 GB for uncompacted structs with inline `Option<ShapeLinearRef>` and
///   `FlexDurationParams`), and **4 consecutive stop times fit inside a single 64-byte CPU cache line**.
/// - Because every trip belonging to a [`RoutePattern`] visits the exact same sequence of places,
///   storing `place_idx` on every stop time is redundant with `pattern.stops[offset].place_idx`.
#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct CompactStopTime {
    /// Scheduled/interpolated arrival time in seconds since midnight, or flex window `start` when `FLAG_IS_FLEX_WINDOW` is set.
    pub arrival_or_start_secs: u32,
    /// Scheduled/interpolated departure time in seconds since midnight, or flex window `end` when `FLAG_IS_FLEX_WINDOW` is set.
    pub departure_or_end_secs: u32,
    /// Cumulative Haversine arc-length offset in meters along the trip's [`ShapePolyline`], or `f32::NAN` if unavailable.
    pub shape_arc_meters: f32,
    /// Index into [`RoutingTimetable::flex_duration_table`], or `u16::MAX` if no GTFS-Flex duration parameters apply.
    pub flex_duration_idx: u16,
    /// Bitflags (`bit 0` = `FLAG_IS_FLEX_WINDOW`).
    pub flags: u16,
}

impl PartialEq for CompactStopTime {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.arrival_or_start_secs == other.arrival_or_start_secs
            && self.departure_or_end_secs == other.departure_or_end_secs
            && ((self.shape_arc_meters.is_nan() && other.shape_arc_meters.is_nan())
                || self.shape_arc_meters == other.shape_arc_meters)
            && self.flex_duration_idx == other.flex_duration_idx
            && self.flags == other.flags
    }
}

impl Eq for CompactStopTime {}

impl CompactStopTime {
    /// Creates a fixed (or interpolated) `CompactStopTime`.
    #[inline]
    pub const fn new_fixed(
        arrival: GtfsTime,
        departure: GtfsTime,
        shape_arc_meters: f32,
        flex_duration_idx: u16,
    ) -> Self {
        Self {
            arrival_or_start_secs: arrival.0,
            departure_or_end_secs: departure.0,
            shape_arc_meters,
            flex_duration_idx,
            flags: 0,
        }
    }

    /// Creates a GTFS-Flex window `CompactStopTime`.
    #[cfg(feature = "gtfs-flex")]
    #[inline]
    pub const fn new_flex_window(
        window: TimeWindow,
        shape_arc_meters: f32,
        flex_duration_idx: u16,
    ) -> Self {
        Self {
            arrival_or_start_secs: window.start.0,
            departure_or_end_secs: window.end.0,
            shape_arc_meters,
            flex_duration_idx,
            flags: FLAG_IS_FLEX_WINDOW,
        }
    }

    /// Returns `true` if this stop time represents a GTFS-Flex time window rather than a fixed timestamp.
    #[inline]
    pub const fn is_flex_window(self) -> bool {
        (self.flags & FLAG_IS_FLEX_WINDOW) != 0
    }

    /// Returns the decoded [`RoutableStopTiming`].
    #[inline]
    pub fn timing(self) -> RoutableStopTiming {
        #[cfg(feature = "gtfs-flex")]
        if self.is_flex_window() {
            return RoutableStopTiming::FlexWindow(TimeWindow {
                start: GtfsTime(self.arrival_or_start_secs),
                end: GtfsTime(self.departure_or_end_secs),
            });
        }
        RoutableStopTiming::Fixed {
            arrival: GtfsTime(self.arrival_or_start_secs),
            departure: GtfsTime(self.departure_or_end_secs),
        }
    }

    /// Returns the resolved shape arc-length offset in meters, or `None` if `shape_arc_meters` is `NaN`.
    #[inline]
    pub fn shape_meters(self) -> Option<f64> {
        if self.shape_arc_meters.is_nan() {
            None
        } else {
            Some(self.shape_arc_meters as f64)
        }
    }
}

impl TemporalStopTime for CompactStopTime {
    #[inline]
    fn arrival_time(&self) -> Option<GtfsTime> {
        if self.is_flex_window() {
            None
        } else {
            Some(GtfsTime(self.arrival_or_start_secs))
        }
    }

    #[inline]
    fn departure_time(&self) -> Option<GtfsTime> {
        if self.is_flex_window() {
            None
        } else {
            Some(GtfsTime(self.departure_or_end_secs))
        }
    }

    #[inline]
    fn flex_window(&self) -> Option<TimeWindow> {
        if self.is_flex_window() {
            Some(TimeWindow {
                start: GtfsTime(self.arrival_or_start_secs),
                end: GtfsTime(self.departure_or_end_secs),
            })
        } else {
            None
        }
    }

    #[inline]
    fn earliest_time(&self) -> Option<GtfsTime> {
        Some(GtfsTime(self.arrival_or_start_secs))
    }

    #[inline]
    fn latest_time(&self) -> Option<GtfsTime> {
        Some(GtfsTime(self.departure_or_end_secs))
    }
}

/// Stop-level boarding and alighting profile entry within a [`RoutePattern`].
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct PatternStopProfile {
    /// Contiguous index of the visited place (`Stop`, `FlexZone`, or `LocationGroup`).
    pub place_idx: PlaceIdx,
    /// Pickup policy at this stop along the pattern.
    pub pickup: BoardingPolicy,
    /// Drop-off policy at this stop along the pattern.
    pub drop_off: BoardingPolicy,
}

/// A unique sequence of [`PatternStopProfile`]s served by one or more trips on a [`Route`].
#[derive(
    Clone, Debug, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct RoutePattern {
    /// Contiguous index of this pattern.
    pub idx: PatternIdx,
    /// Contiguous index of the archive/feed that owns this pattern.
    pub feed_idx: FeedIdx,
    /// Contiguous index of the parent [`Route`].
    pub route_idx: RouteIdx,
    /// Ordered stop sequence and boarding/alighting policies for this pattern.
    pub stops: Vec<PatternStopProfile>,
    /// Contiguous [`TripIdx`]s belonging to this pattern, sorted by earliest departure at stop 0.
    pub trips: Vec<TripIdx>,
}

/// A validated, interpolated trip in the [`RoutingTimetable`] referencing a contiguous slice
/// of [`CompactStopTime`]s in [`RoutingTimetable::stop_times`].
#[derive(
    Clone, Debug, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct RoutableTrip {
    /// Contiguous index of this trip.
    pub idx: TripIdx,
    /// Contiguous index of the archive/feed that owns this trip.
    pub feed_idx: FeedIdx,
    /// Original GTFS `TripId`.
    pub id: TripId,
    /// Contiguous index of the parent [`Route`].
    pub route_idx: RouteIdx,
    /// Contiguous index of the [`RoutePattern`] this trip follows.
    pub pattern_idx: PatternIdx,
    /// Contiguous index of the [`ServiceCalendar`] governing when this trip runs.
    pub service_idx: ServiceIdx,
    /// Optional contiguous index of the [`ShapePolyline`] traced by this trip.
    pub shape_idx: Option<ShapeIdx>,
    /// Optional trip headsign.
    pub headsign: Option<String>,
    /// Start index into [`RoutingTimetable::stop_times`] where this trip's `stop_count` entries begin.
    pub stop_times_offset: u32,
    /// Number of stops on this trip (always equal to `pattern.stops.len()`).
    pub stop_count: u16,
    /// Optional frequency headway windows.
    pub frequencies: Vec<FrequencyWindow>,
}

impl RoutableTrip {
    /// Returns the index range into [`RoutingTimetable::stop_times`] for this trip.
    #[inline]
    pub const fn stop_times_range(&self) -> Range<usize> {
        let start = self.stop_times_offset as usize;
        start..(start + self.stop_count as usize)
    }
}

/// Reverse-index entry mapping a [`PlaceIdx`] to a [`PatternIdx`] and its 0-based position
/// within `pattern.stops`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct PlacePatternOccurrence {
    /// Pattern visiting the place.
    pub pattern_idx: PatternIdx,
    /// 0-based offset into `pattern.stops` (and `timetable.trip_stop_times(trip_idx)`).
    pub stop_offset_in_pattern: u16,
}

/// Pre-indexed stop-to-stop transfer or pathway edge between two [`PlaceIdx`]s.
#[derive(
    Clone, Copy, Debug, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct IndexedTransfer {
    /// Origin place index.
    pub from_place: PlaceIdx,
    /// Destination place index.
    pub to_place: PlaceIdx,
    /// Transfer type (`Recommended`, `Timed`, `MinTime`, `Impossible`, `StayOnBoard`, etc.).
    pub transfer_type: TransferType,
    /// Minimum transfer time or pathway traversal time in seconds.
    pub min_time_seconds: Option<u32>,
    /// Optional horizontal pathway length in meters.
    pub distance_meters: Option<f32>,
}

/// Spatial index entry associating a [`PlaceIdx`] with its WGS84 bounding box inside an [`rstar::RTree`].
#[derive(
    Clone, Copy, Debug, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct PlaceSpatialEntry {
    /// Contiguous index of the indexed [`StopPlace`].
    pub place_idx: PlaceIdx,
    /// Bounding box of the place (`[min_lon, min_lat]` to `[max_lon, max_lat]`).
    pub bbox: BoundingBox,
}

impl RTreeObject for PlaceSpatialEntry {
    type Envelope = AABB<[f64; 2]>;

    #[inline]
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.bbox.min_lon, self.bbox.min_lat],
            [self.bbox.max_lon, self.bbox.max_lat],
        )
    }
}

/// Active service evaluation window for a local `(date, time_of_day)` query, supporting both
/// current-day schedules and previous-day overnight trips (`>= 24:00:00` on `date - 1`).
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct ActiveServiceWindow {
    /// Local calendar date of the query.
    #[rkyv(with = crate::primitives::RkyvNaiveDate)]
    pub date: NaiveDate,
    /// Local time of day on `date` (`0..86_400` seconds).
    pub time_of_day: GtfsTime,
    /// Boolean mask indexed by [`ServiceIdx`] indicating services active on `date`.
    pub today_active: Vec<bool>,
    /// Previous calendar date (`date - 1 day`).
    #[rkyv(with = crate::primitives::RkyvNaiveDate)]
    pub yesterday_date: NaiveDate,
    /// Query time expressed on `yesterday_date`'s service clock (`time_of_day + 86_400` seconds).
    pub yesterday_overnight_time: GtfsTime,
    /// Boolean mask indexed by [`ServiceIdx`] indicating services active on `yesterday_date`.
    pub yesterday_active: Vec<bool>,
}

/// Earliest boardable trip or frequency/flex departure returned by
/// [`RoutingTimetable::earliest_boardable_trip`] or [`RoutingTimetable::earliest_boardable_trip_with_overnight`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct BoardableDeparture {
    /// Contiguous index of the matched [`RoutableTrip`].
    pub trip_idx: TripIdx,
    /// Earliest time the rider can board at `stop_offset` (in the service day's `GtfsTime` clock).
    pub departure_time: GtfsTime,
    /// Signed offset in seconds relative to the template trip's stop times:
    /// - `0` for fixed-schedule trips, materialized frequency trips, or flex windows,
    /// - `> 0` (or `< 0`) when boarding a non-materialized `frequencies.txt` headway departure.
    pub frequency_offset_secs: i64,
    /// `true` if this departure belongs to yesterday's service day (`date - 1` with GTFS time `>= 24:00:00`).
    pub is_overnight_from_yesterday: bool,
}

impl BoardableDeparture {
    /// Computes the arrival time at a downstream `to_stop_offset` along the same trip,
    /// automatically applying `self.frequency_offset_secs`.
    pub fn arrival_time_at(&self, timetable: &RoutingTimetable, to_stop_offset: usize) -> Option<GtfsTime> {
        let st = timetable.trip_stop_times(self.trip_idx).get(to_stop_offset)?;
        let base = st.earliest_time()?;
        Some(base.add_signed_seconds(self.frequency_offset_secs))
    }

    /// Normalizes `self.departure_time` back to the current calendar day's clock
    /// (subtracting `86_400` seconds when `self.is_overnight_from_yesterday` is `true`).
    #[inline]
    pub const fn normalized_query_day_departure(&self) -> GtfsTime {
        if self.is_overnight_from_yesterday {
            GtfsTime(self.departure_time.0.saturating_sub(86_400))
        } else {
            self.departure_time
        }
    }
}

/// Metadata and contiguous index spans for a single GTFS archive inside a merged [`RoutingTimetable`].
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FeedMetadata {
    /// Contiguous index of this feed (`0..feeds.len()`).
    pub idx: FeedIdx,
    /// Unique archive identifier (e.g. `"mdb-412"` or `"rtd-denver"`).
    pub id: FeedId,
    /// Agencies defined in this archive's `agency.txt`.
    pub agencies: Vec<Agency>,
    /// Primary parsed IANA [`jiff::tz::TimeZone`] for this feed (derived from the first agency with a valid timezone).
    #[serde(
        serialize_with = "crate::model::agency::serialize_option_timezone",
        deserialize_with = "crate::model::agency::deserialize_option_timezone",
        default
    )]
    #[rkyv(with = crate::model::agency::RkyvOptionTimeZone)]
    pub primary_timezone: Option<TimeZone>,
    /// Contiguous range of [`PlaceIdx`]s belonging to this feed.
    pub places_span: Range<u32>,
    /// Contiguous range of [`RouteIdx`]s belonging to this feed.
    pub routes_span: Range<u32>,
    /// Contiguous range of [`PatternIdx`]s belonging to this feed.
    pub patterns_span: Range<u32>,
    /// Contiguous range of [`TripIdx`]s belonging to this feed.
    pub trips_span: Range<u32>,
    /// Contiguous range of [`ServiceIdx`]s belonging to this feed.
    pub services_span: Range<u32>,
    /// Contiguous range of [`ShapeIdx`]s belonging to this feed.
    pub shapes_span: Range<u32>,
    /// Structured warnings emitted during Phase 2 and Phase 3 validation for this feed.
    pub warnings: Vec<ValidationWarning>,
    /// Summary counts of dropped or repaired entities for this feed.
    pub dropped_counts: DroppedEntityCounts,
}

/// Borrowed view of the complete, globally unambiguous lineage of a [`TripIdx`]:
/// `(FeedId, AgencyId, RouteId, TripId)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullyQualifiedTripId<'a> {
    /// Contiguous feed index.
    pub feed_idx: FeedIdx,
    /// Archive / feed identifier.
    pub feed_id: &'a FeedId,
    /// Resolved agency identifier (`Route::agency_id`, or the sole agency's `id` in single-agency feeds).
    pub agency_id: Option<&'a AgencyId>,
    /// Resolved agency name, if available.
    pub agency_name: Option<&'a str>,
    /// Route identifier (`routes.txt: route_id`).
    pub route_id: &'a RouteId,
    /// Trip identifier (`trips.txt: trip_id`).
    pub trip_id: &'a TripId,
}

/// Borrowed view of the globally unambiguous lineage of a [`RouteIdx`]:
/// `(FeedId, AgencyId, RouteId)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullyQualifiedRouteId<'a> {
    /// Contiguous feed index.
    pub feed_idx: FeedIdx,
    /// Archive / feed identifier.
    pub feed_id: &'a FeedId,
    /// Resolved agency identifier.
    pub agency_id: Option<&'a AgencyId>,
    /// Route identifier.
    pub route_id: &'a RouteId,
}

/// Kind of routable place referenced by a [`PlaceIdx`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceIdKind<'a> {
    /// Fixed stop from `stops.txt`.
    Stop(&'a StopId),
    /// Flexible GeoJSON zone from `locations.geojson` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    FlexZone(&'a LocationId),
    /// Location group from `location_groups.txt` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    LocationGroup(&'a LocationGroupId),
}

impl<'a> PlaceIdKind<'a> {
    /// Returns the raw string slice of the underlying identifier.
    #[inline]
    pub fn as_str(self) -> &'a str {
        match self {
            PlaceIdKind::Stop(id) => id.as_str(),
            #[cfg(feature = "gtfs-flex")]
            PlaceIdKind::FlexZone(id) => id.as_str(),
            #[cfg(feature = "gtfs-flex")]
            PlaceIdKind::LocationGroup(id) => id.as_str(),
        }
    }
}

/// Borrowed view of the globally unambiguous lineage of a [`PlaceIdx`]:
/// `(FeedId, StopId | LocationId | LocationGroupId)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullyQualifiedPlaceId<'a> {
    /// Contiguous feed index.
    pub feed_idx: FeedIdx,
    /// Archive / feed identifier.
    pub feed_id: &'a FeedId,
    /// Typed place identifier (`Stop`, `FlexZone`, or `LocationGroup`).
    pub place_id: PlaceIdKind<'a>,
}

fn build_place_spatial_index(places: &[StopPlace]) -> RTree<PlaceSpatialEntry> {
    let entries: Vec<PlaceSpatialEntry> = places
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            p.bounding_box().map(|bbox| PlaceSpatialEntry {
                place_idx: PlaceIdx::from_usize(i),
                bbox,
            })
        })
        .collect();
    RTree::bulk_load(entries)
}

fn serialize_spatial_index<S: serde::Serializer>(
    tree: &RTree<PlaceSpatialEntry>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    let mut entries: Vec<PlaceSpatialEntry> = tree.iter().copied().collect();
    entries.sort_by_key(|e| e.place_idx);
    entries.serialize(serializer)
}

fn deserialize_spatial_index<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<RTree<PlaceSpatialEntry>, D::Error> {
    use serde::Deserialize;
    let entries = Vec::<PlaceSpatialEntry>::deserialize(deserializer)?;
    Ok(RTree::bulk_load(entries))
}

/// `rkyv::with` wrapper that serializes `rstar::RTree<PlaceSpatialEntry>` as a sorted `Vec<PlaceSpatialEntry>`
/// and bulk-loads the R-tree on deserialization.
pub struct RkyvSpatialIndex;

impl rkyv::with::ArchiveWith<RTree<PlaceSpatialEntry>> for RkyvSpatialIndex {
    type Archived = rkyv::vec::ArchivedVec<ArchivedPlaceSpatialEntry>;
    type Resolver = rkyv::vec::VecResolver;

    fn resolve_with(
        field: &RTree<PlaceSpatialEntry>,
        resolver: Self::Resolver,
        out: rkyv::Place<Self::Archived>,
    ) {
        let mut entries: Vec<PlaceSpatialEntry> = field.iter().copied().collect();
        entries.sort_by_key(|e| e.place_idx);
        rkyv::vec::ArchivedVec::resolve_from_slice(&entries, resolver, out);
    }
}

impl<S: rkyv::rancor::Fallible + rkyv::ser::Allocator + rkyv::ser::Writer + ?Sized>
    rkyv::with::SerializeWith<RTree<PlaceSpatialEntry>, S> for RkyvSpatialIndex
{
    fn serialize_with(
        field: &RTree<PlaceSpatialEntry>,
        serializer: &mut S,
    ) -> Result<Self::Resolver, S::Error> {
        let mut entries: Vec<PlaceSpatialEntry> = field.iter().copied().collect();
        entries.sort_by_key(|e| e.place_idx);
        rkyv::vec::ArchivedVec::serialize_from_slice(&entries, serializer)
    }
}

impl<D>
    rkyv::with::DeserializeWith<
        rkyv::vec::ArchivedVec<ArchivedPlaceSpatialEntry>,
        RTree<PlaceSpatialEntry>,
        D,
    > for RkyvSpatialIndex
where
    D: rkyv::rancor::Fallible + ?Sized,
    D::Error: rkyv::rancor::Source,
{
    fn deserialize_with(
        field: &rkyv::vec::ArchivedVec<ArchivedPlaceSpatialEntry>,
        deserializer: &mut D,
    ) -> Result<RTree<PlaceSpatialEntry>, D::Error> {
        let entries: Vec<PlaceSpatialEntry> = rkyv::Deserialize::deserialize(field, deserializer)?;
        Ok(RTree::bulk_load(entries))
    }
}

/// Phase 3: RAPTOR-style contiguous CSR timetable, multi-feed archive registry, and spatial/shape
/// index ready for continental-scale vectorized multimodal A* graph search.
#[derive(
    Clone, Debug, Default, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct RoutingTimetable {
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
    /// Use [`RoutingTimetable::trip_stop_times`] or `timetable.stop_times[trip.stop_times_range()]`
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

impl RoutingTimetable {
    /// Creates a new [`RoutingTimetableBuilder`] for merging one or more [`ValidatedFeed`]s
    /// into a single contiguous national/regional [`RoutingTimetable`].
    #[inline]
    pub fn builder() -> RoutingTimetableBuilder {
        RoutingTimetableBuilder::new()
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
    #[inline]
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

    // --- O(1) Reverse Lookups (Global Index -> Fully-Qualified Lineage) ---

    /// Resolves a [`TripIdx`] in $O(1)$ time (three direct slice reads, zero hash lookups) into its
    /// [`FullyQualifiedTripId`]: `(FeedId, AgencyId, RouteId, TripId)`.
    pub fn resolve_trip_id(&self, trip_idx: TripIdx) -> Option<FullyQualifiedTripId<'_>> {
        let trip = self.trips.get(trip_idx.index())?;
        let route = self.routes.get(trip.route_idx.index())?;
        let feed = self.feeds.get(trip.feed_idx.index())?;

        let (agency_id, agency_name) = resolve_route_agency(route, &feed.agencies);

        Some(FullyQualifiedTripId {
            feed_idx: trip.feed_idx,
            feed_id: &feed.id,
            agency_id,
            agency_name,
            route_id: &route.id,
            trip_id: &trip.id,
        })
    }

    /// Resolves a [`RouteIdx`] in $O(1)$ time into its [`FullyQualifiedRouteId`]: `(FeedId, AgencyId, RouteId)`.
    pub fn resolve_route_id(&self, route_idx: RouteIdx) -> Option<FullyQualifiedRouteId<'_>> {
        let route = self.routes.get(route_idx.index())?;
        let feed_idx = *self.route_feed.get(route_idx.index())?;
        let feed = self.feeds.get(feed_idx.index())?;
        let (agency_id, _) = resolve_route_agency(route, &feed.agencies);

        Some(FullyQualifiedRouteId {
            feed_idx,
            feed_id: &feed.id,
            agency_id,
            route_id: &route.id,
        })
    }

    /// Resolves a [`PlaceIdx`] in $O(1)$ time into its [`FullyQualifiedPlaceId`]:
    /// `(FeedId, StopId | LocationId | LocationGroupId)`.
    pub fn resolve_place_id(&self, place_idx: PlaceIdx) -> Option<FullyQualifiedPlaceId<'_>> {
        let place = self.places.get(place_idx.index())?;
        let feed_idx = *self.place_feed.get(place_idx.index())?;
        let feed = self.feeds.get(feed_idx.index())?;

        let place_id = match place {
            StopPlace::Stop(s) => PlaceIdKind::Stop(&s.id),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::FlexZone(z) => PlaceIdKind::FlexZone(&z.id),
            #[cfg(feature = "gtfs-flex")]
            StopPlace::LocationGroup(g) => PlaceIdKind::LocationGroup(&g.id),
        };

        Some(FullyQualifiedPlaceId {
            feed_idx,
            feed_id: &feed.id,
            place_id,
        })
    }

    // --- O(1) Forward Lookups (Fully-Qualified / Scoped ID -> Global Index) ---

    /// Looks up a [`PlaceIdx`] by `(FeedId, StopId)` in $O(1)$ time.
    #[inline]
    pub fn lookup_stop(&self, feed_id: &FeedId, stop_id: &StopId) -> Option<PlaceIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_stop_to_place
            .get(&(feed_idx, stop_id.clone()))
            .copied()
    }

    /// Looks up a [`PlaceIdx`] by `(FeedId, LocationId)` in $O(1)$ time (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[inline]
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
    #[inline]
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
    #[inline]
    pub fn lookup_route(&self, feed_id: &FeedId, route_id: &RouteId) -> Option<RouteIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_route_to_idx
            .get(&(feed_idx, route_id.clone()))
            .copied()
    }

    /// Looks up a [`TripIdx`] by `(FeedId, TripId)` in $O(1)$ time.
    #[inline]
    pub fn lookup_trip(&self, feed_id: &FeedId, trip_id: &TripId) -> Option<TripIdx> {
        let &feed_idx = self.feed_id_to_idx.get(feed_id)?;
        self.scoped_trip_to_idx
            .get(&(feed_idx, trip_id.clone()))
            .copied()
    }

    /// Returns a boolean mask indexed by [`ServiceIdx`] indicating which services are active on `date`.
    pub fn active_services_mask(&self, date: NaiveDate) -> Vec<bool> {
        self.services.iter().map(|s| s.is_active_on(date)).collect()
    }

    /// Evaluates active services for a local `(date, time_of_day)` query, including previous-day
    /// (`date - 1`) services that may still be running overnight with GTFS timestamps `>= 24:00:00`
    /// (`time_of_day + 86_400` seconds).
    pub fn active_services_at_time(
        &self,
        date: NaiveDate,
        time_of_day: GtfsTime,
    ) -> ActiveServiceWindow {
        let today_active = self.active_services_mask(date);
        let yesterday_date = date.checked_sub_days(Days::new(1)).unwrap_or(date);
        let yesterday_active = self.active_services_mask(yesterday_date);
        let yesterday_overnight_time = time_of_day.add_seconds(86_400);
        ActiveServiceWindow {
            date,
            time_of_day,
            today_active,
            yesterday_date,
            yesterday_overnight_time,
            yesterday_active,
        }
    }

    /// Evaluates active services for `feed_idx` at a unified UTC [`jiff::Timestamp`] by converting
    /// the instant into the feed's primary IANA [`jiff::tz::TimeZone`] (from `agency.txt`).
    pub fn active_services_for_feed_at_timestamp(
        &self,
        feed_idx: FeedIdx,
        timestamp: jiff::Timestamp,
    ) -> Option<ActiveServiceWindow> {
        let feed = self.feeds.get(feed_idx.index())?;
        let tz = feed.primary_timezone.as_ref()?;
        let zdt = timestamp.to_zoned(tz.clone());
        let date = NaiveDate::from_ymd_opt(
            i32::from(zdt.year()),
            u32::try_from(zdt.month()).ok()?,
            u32::try_from(zdt.day()).ok()?,
        )?;
        let time_of_day = GtfsTime::from_hms(
            u32::try_from(zdt.hour()).ok()?,
            u32::try_from(zdt.minute()).ok()?,
            u32::try_from(zdt.second()).ok()?,
        );
        Some(self.active_services_at_time(date, time_of_day))
    }

    /// Finds the earliest boardable trip on `pattern_idx` at `stop_offset` departing at or after `after_time`
    /// among trips whose `service_idx` is enabled in `active_services`.
    ///
    /// Seamlessly handles:
    /// 1. **Fixed-schedule trips** (and materialized frequency trips) via `partition_point` binary search
    ///    when the pattern consists purely of non-frequency fixed-schedule trips,
    /// 2. **Headway-based `frequencies.txt` trips** by computing the first headway departure `>= after_time`
    ///    within each `FrequencyWindow`, and
    /// 3. **GTFS-Flex `FlexWindow` trips** where `after_time <= window.end` (boarding at `max(after_time, window.start)`).
    pub fn earliest_boardable_trip(
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

        // Fast path: if no trip on this pattern has unexpanded frequency windows or flex windows,
        // `pattern.trips` is monotonically sorted by stop times, so we can binary-search with `partition_point`.
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
                // Frequency-based template trip:
                // Let `t0` be the template trip's departure at stop 0, and `ts` be its departure at `stop_offset`.
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

                    // First vehicle departure at `stop_offset` occurs at `win_start + travel_offset_from_start`.
                    let first_dep_at_stop = win_start + travel_offset_from_start;
                    let target = after_time.0 as i64;

                    let k = if target <= first_dep_at_stop {
                        0
                    } else {
                        (target - first_dep_at_stop + headway - 1) / headway
                    };

                    let vehicle_start_at_stop0 = win_start + k * headway;
                    // Per GTFS frequencies.txt spec, `end_time` is the time at which service changes or ceases at the first stop.
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
    pub fn earliest_boardable_trip_with_overnight(
        &self,
        pattern_idx: PatternIdx,
        stop_offset: usize,
        window: &ActiveServiceWindow,
    ) -> Option<BoardableDeparture> {
        let today_cand = self.earliest_boardable_trip(
            pattern_idx,
            stop_offset,
            window.time_of_day,
            &window.today_active,
        );
        let overnight_cand = self
            .earliest_boardable_trip(
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
                    if let Some(sc) = s.coord {
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
                                    .coord
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

    /// Serializes this [`RoutingTimetable`] into a compact `rkyv` byte vector.
    pub fn to_rkyv_bytes(&self) -> Result<Vec<u8>, RoutingError> {
        let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        Ok(aligned.into_vec())
    }

    /// Validates and returns a zero-copy [`ArchivedRoutingTimetable`] reference directly from a
    /// 16-byte-aligned `rkyv` byte slice (such as an [`rkyv::util::AlignedVec`] or memory-mapped buffer)
    /// without allocating or copying the timetable's vectors.
    pub fn access_rkyv_bytes(bytes: &[u8]) -> Result<&ArchivedRoutingTimetable, RoutingError> {
        rkyv::access::<ArchivedRoutingTimetable, rkyv::rancor::Error>(bytes)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Deserializes a [`RoutingTimetable`] from an `rkyv` byte slice (validating the archive via `bytecheck`).
    pub fn from_rkyv_bytes(bytes: &[u8]) -> Result<Self, RoutingError> {
        let mut aligned = rkyv::util::AlignedVec::<16>::with_capacity(bytes.len());
        aligned.extend_from_slice(bytes);
        rkyv::from_bytes::<Self, rkyv::rancor::Error>(&aligned)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Writes this [`RoutingTimetable`] as a binary `rkyv` snapshot to `writer`.
    pub fn write_rkyv<W: Write>(&self, mut writer: W) -> Result<(), RoutingError> {
        let aligned = rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        writer
            .write_all(&aligned)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))
    }

    /// Reads a [`RoutingTimetable`] from a binary `rkyv` snapshot `reader`.
    pub fn read_rkyv<R: Read>(mut reader: R) -> Result<Self, RoutingError> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        Self::from_rkyv_bytes(&bytes)
    }

    /// Saves this [`RoutingTimetable`] to a file on disk in `rkyv` format.
    pub fn save_rkyv(&self, path: impl AsRef<Path>) -> Result<(), RoutingError> {
        let file = File::create(path).map_err(|e| RoutingError::Snapshot(e.to_string()))?;
        let writer = BufWriter::new(file);
        self.write_rkyv(writer)
    }

    /// Loads a [`RoutingTimetable`] from an `rkyv` snapshot file on disk.
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

#[cfg(feature = "gtfs-flex")]
fn flex_zone_within_radius(
    zone: &crate::model::FlexLocation,
    target: Coord,
    radius_meters: f64,
) -> bool {
    let check_ring = |ring: &[Coord]| -> bool {
        if ring.iter().any(|&v| v.haversine_meters(target) <= radius_meters) {
            return true;
        }
        let cos_lat = target.lat.to_radians().cos().max(1e-6);
        for seg in ring.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let ax = (a.lon - target.lon) * cos_lat;
            let ay = a.lat - target.lat;
            let bx = (b.lon - target.lon) * cos_lat;
            let by = b.lat - target.lat;
            let dx = bx - ax;
            let dy = by - ay;
            let len_sq = dx * dx + dy * dy;
            let t = if len_sq > 0.0 {
                (-(ax * dx + ay * dy) / len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let closest = Coord::lerp(a, b, t);
            if closest.haversine_meters(target) <= radius_meters {
                return true;
            }
        }
        false
    };

    match &zone.geometry {
        crate::raw::RawFlexGeometry::Polygon(rings) => rings.iter().any(|r| check_ring(r)),
        crate::raw::RawFlexGeometry::MultiPolygon(polys) => {
            polys.iter().any(|rings| rings.iter().any(|r| check_ring(r)))
        }
    }
}

fn resolve_route_agency<'a>(
    route: &'a Route,
    agencies: &'a [Agency],
) -> (Option<&'a AgencyId>, Option<&'a str>) {
    if let Some(aid) = &route.agency_id {
        if let Some(ag) = agencies.iter().find(|a| a.id.as_ref() == Some(aid)) {
            return (Some(aid), Some(ag.name.as_str()));
        }
        return (Some(aid), None);
    }
    if let Some(ag) = agencies.first() {
        return (ag.id.as_ref(), Some(ag.name.as_str()));
    }
    (None, None)
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PatternKey {
    feed_idx: FeedIdx,
    route_idx: RouteIdx,
    places: Vec<PlaceIdx>,
    pickup_methods: Vec<i16>,
    drop_off_methods: Vec<i16>,
}

fn pickup_code(p: &BoardingPolicy) -> i16 {
    match p.method {
        crate::enums::PickupDropOffType::Regular => 0,
        crate::enums::PickupDropOffType::NotAvailable => 1,
        crate::enums::PickupDropOffType::ArrangeByPhone => 2,
        crate::enums::PickupDropOffType::CoordinateWithDriver => 3,
        crate::enums::PickupDropOffType::Unknown(c) => c,
    }
}

/// Multi-feed builder that incrementally compiles and merges one or more [`ValidatedFeed`]s
/// into a single contiguous [`RoutingTimetable`].
///
/// Designed for HPC workflows where each GTFS archive is parsed and validated into a
/// [`ValidatedFeed`], appended via [`RoutingTimetableBuilder::add_feed`], and immediately dropped
/// so peak memory never exceeds the compact CSR arena.
#[derive(Default)]
pub struct RoutingTimetableBuilder {
    timetable: RoutingTimetable,
    pattern_lookup: HashMap<PatternKey, PatternIdx>,
    validation_policy: ValidationPolicy,
    materialize_frequency_trips: bool,
    #[cfg(feature = "gtfs-flex")]
    flex_duration_lookup: HashMap<[u64; 4], u16>,
}

impl RoutingTimetableBuilder {
    /// Creates an empty [`RoutingTimetableBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the [`ValidationPolicy`] used when interpolating and packing trips in Phase 3.
    ///
    /// When set to [`ValidationPolicy::DropInvalidTripsWithWarning`], trips with 0 or 1 stops,
    /// missing boundary timestamps (`UninterpolatableTripBoundary`), or decreasing stop times
    /// (`DecreasingStopTimes`) are dropped and recorded on [`FeedMetadata::warnings`] and
    /// [`FeedMetadata::dropped_counts`] instead of aborting the build.
    pub fn with_validation_policy(mut self, policy: ValidationPolicy) -> Self {
        self.validation_policy = policy;
        self
    }

    /// Configures whether headway-based `frequencies.txt` trips should be materialized into concrete
    /// [`RoutableTrip`] instances (with `CompactStopTime` entries shifted for each headway departure
    /// and `frequencies` cleared on the expanded trips).
    ///
    /// Note that [`RoutingTimetable::earliest_boardable_trip`] supports both materialized and
    /// unexpanded frequency trips automatically.
    pub fn with_materialize_frequencies(mut self, materialize: bool) -> Self {
        self.materialize_frequency_trips = materialize;
        self
    }

    /// Appends a [`ValidatedFeed`] under `feed_id` into the contiguous [`RoutingTimetable`] arena,
    /// returning its assigned [`FeedIdx`].
    pub fn add_feed(
        &mut self,
        feed_id: impl Into<FeedId>,
        mut feed: ValidatedFeed,
    ) -> Result<FeedIdx, RoutingError> {
        let feed_id = feed_id.into();
        let feed_idx = FeedIdx::from_usize(self.timetable.feeds.len());
        self.timetable
            .feed_id_to_idx
            .insert(feed_id.clone(), feed_idx);

        let places_start = self.timetable.places.len() as u32;
        let routes_start = self.timetable.routes.len() as u32;
        let patterns_start = self.timetable.patterns.len() as u32;
        let trips_start = self.timetable.trips.len() as u32;
        let services_start = self.timetable.services.len() as u32;
        let shapes_start = self.timetable.shapes.len() as u32;

        // Feed-local lookup maps used while wiring this feed's entities
        let mut local_stop_to_place: HashMap<StopId, PlaceIdx> =
            HashMap::with_capacity(feed.stops.len());
        #[cfg(feature = "gtfs-flex")]
        let mut local_loc_to_place: HashMap<LocationId, PlaceIdx> =
            HashMap::with_capacity(feed.locations.len());
        #[cfg(feature = "gtfs-flex")]
        let mut local_grp_to_place: HashMap<LocationGroupId, PlaceIdx> =
            HashMap::with_capacity(feed.location_groups.len());

        // 1. Append Places (Stops, FlexLocations, LocationGroups)
        let mut sorted_stops: Vec<_> = feed.stops.into_values().collect();
        sorted_stops.sort_by(|a, b| a.id.cmp(&b.id));
        for stop in sorted_stops {
            let idx = PlaceIdx::from_usize(self.timetable.places.len());
            local_stop_to_place.insert(stop.id.clone(), idx);
            self.timetable
                .scoped_stop_to_place
                .insert((feed_idx, stop.id.clone()), idx);
            self.timetable.stop_id_to_place.insert(stop.id.clone(), idx);
            self.timetable.places.push(StopPlace::Stop(stop));
            self.timetable.place_feed.push(feed_idx);
            self.timetable.place_to_patterns.push(Vec::new());
            self.timetable.place_to_transfers.push(Vec::new());
            self.timetable.group_to_member_places.push(Vec::new());
        }

        #[cfg(feature = "gtfs-flex")]
        {
            let mut sorted_locs: Vec<_> = feed.locations.into_values().collect();
            sorted_locs.sort_by(|a, b| a.id.cmp(&b.id));
            for loc in sorted_locs {
                let idx = PlaceIdx::from_usize(self.timetable.places.len());
                local_loc_to_place.insert(loc.id.clone(), idx);
                self.timetable
                    .scoped_location_to_place
                    .insert((feed_idx, loc.id.clone()), idx);
                self.timetable
                    .location_id_to_place
                    .insert(loc.id.clone(), idx);
                self.timetable.places.push(StopPlace::FlexZone(loc));
                self.timetable.place_feed.push(feed_idx);
                self.timetable.place_to_patterns.push(Vec::new());
                self.timetable.place_to_transfers.push(Vec::new());
                self.timetable.group_to_member_places.push(Vec::new());
            }

            let mut sorted_groups: Vec<_> = feed.location_groups.into_values().collect();
            sorted_groups.sort_by(|a, b| a.id.cmp(&b.id));
            for grp in sorted_groups {
                let idx = PlaceIdx::from_usize(self.timetable.places.len());
                local_grp_to_place.insert(grp.id.clone(), idx);
                self.timetable
                    .scoped_location_group_to_place
                    .insert((feed_idx, grp.id.clone()), idx);
                self.timetable
                    .location_group_id_to_place
                    .insert(grp.id.clone(), idx);

                let mut member_indices = Vec::with_capacity(grp.members.len());
                for m in &grp.members {
                    match m {
                        crate::model::PlaceMember::Stop(s) => {
                            if let Some(&pidx) = local_stop_to_place.get(&s.id) {
                                member_indices.push(pidx);
                            }
                        }
                        crate::model::PlaceMember::FlexZone(z) => {
                            if let Some(&pidx) = local_loc_to_place.get(&z.id) {
                                member_indices.push(pidx);
                            }
                        }
                    }
                }

                self.timetable.places.push(StopPlace::LocationGroup(grp));
                self.timetable.place_feed.push(feed_idx);
                self.timetable.place_to_patterns.push(Vec::new());
                self.timetable.place_to_transfers.push(Vec::new());
                self.timetable.group_to_member_places.push(member_indices);
            }
        }

        let places_end = self.timetable.places.len() as u32;

        // 2. Append Transfers & Pathways for this feed's places
        for place_u32 in places_start..places_end {
            let i = place_u32 as usize;
            let from_place = PlaceIdx::new(place_u32);
            if let Some(stop) = self.timetable.places[i].as_stop() {
                let mut edges = Vec::with_capacity(stop.transfers.len() + stop.pathways.len());
                for tr in &stop.transfers {
                    if let Some(&to_place) = local_stop_to_place.get(&tr.to_stop_id) {
                        edges.push(IndexedTransfer {
                            from_place,
                            to_place,
                            transfer_type: tr.transfer_type,
                            min_time_seconds: tr.min_transfer_time,
                            distance_meters: None,
                        });
                    }
                }
                for pw in &stop.pathways {
                    if let Some(&to_place) = local_stop_to_place.get(&pw.to_stop_id) {
                        edges.push(IndexedTransfer {
                            from_place,
                            to_place,
                            transfer_type: TransferType::MinTime,
                            min_time_seconds: pw.traversal_time,
                            distance_meters: pw.length,
                        });
                    }
                }
                self.timetable.place_to_transfers[i] = edges;
            }
        }

        // 3. Append Routes
        let mut sorted_routes: Vec<Route> = feed.routes.into_values().collect();
        sorted_routes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut local_route_to_idx: HashMap<RouteId, RouteIdx> =
            HashMap::with_capacity(sorted_routes.len());
        for route in sorted_routes {
            let ridx = RouteIdx::from_usize(self.timetable.routes.len());
            local_route_to_idx.insert(route.id.clone(), ridx);
            self.timetable
                .scoped_route_to_idx
                .insert((feed_idx, route.id.clone()), ridx);
            self.timetable
                .route_id_to_idx
                .insert(route.id.clone(), ridx);
            self.timetable.routes.push(route);
            self.timetable.route_feed.push(feed_idx);
            self.timetable.route_to_patterns.push(Vec::new());
        }
        let routes_end = self.timetable.routes.len() as u32;

        // 4. Append Services
        let mut sorted_services: Vec<ServiceCalendar> = feed.services.into_values().collect();
        sorted_services.sort_by(|a, b| a.id.cmp(&b.id));
        let mut local_service_to_idx: HashMap<ServiceId, ServiceIdx> =
            HashMap::with_capacity(sorted_services.len());
        for svc in sorted_services {
            let sidx = ServiceIdx::from_usize(self.timetable.services.len());
            local_service_to_idx.insert(svc.id.clone(), sidx);
            self.timetable
                .scoped_service_to_idx
                .insert((feed_idx, svc.id.clone()), sidx);
            self.timetable
                .service_id_to_idx
                .insert(svc.id.clone(), sidx);
            self.timetable.services.push(svc);
            self.timetable.service_feed.push(feed_idx);
        }

        // 5. Append Shapes
        let mut sorted_shapes: Vec<Arc<ShapePolyline>> = feed.shapes.into_values().collect();
        sorted_shapes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut local_shape_to_idx: HashMap<ShapeId, ShapeIdx> =
            HashMap::with_capacity(sorted_shapes.len());
        for shp in sorted_shapes {
            let shidx = ShapeIdx::from_usize(self.timetable.shapes.len());
            local_shape_to_idx.insert(shp.id.clone(), shidx);
            self.timetable
                .scoped_shape_to_idx
                .insert((feed_idx, shp.id.clone()), shidx);
            self.timetable.shape_id_to_idx.insert(shp.id.clone(), shidx);
            self.timetable.shapes.push(shp);
            self.timetable.shape_feed.push(feed_idx);
        }
        let shapes_end = self.timetable.shapes.len() as u32;

        // 6. Interpolate Trips & Append to Flat CSR StopTimes Arena
        let mut raw_trips: Vec<Trip> = feed.trips.into_values().collect();
        raw_trips.sort_by(|a, b| a.id.cmp(&b.id));

        for mut trip in raw_trips {
            if self.validation_policy == ValidationPolicy::DropInvalidTripsWithWarning {
                if trip.stop_times.is_empty() {
                    feed.dropped_counts.empty_trips += 1;
                    feed.warnings.push(ValidationWarning::DroppedTrip {
                        trip_id: trip.id.to_string(),
                        reason: "trip has 0 stop_times rows".to_owned(),
                    });
                    continue;
                }
                if trip.stop_times.len() == 1 {
                    feed.dropped_counts.single_stop_trips += 1;
                    feed.warnings.push(ValidationWarning::DroppedTrip {
                        trip_id: trip.id.to_string(),
                        reason: "trip has only 1 stop_times row".to_owned(),
                    });
                    continue;
                }
            }

            let service_idx = match local_service_to_idx.get(&trip.service_id).copied() {
                Some(idx) => idx,
                None => {
                    let sidx = ServiceIdx::from_usize(self.timetable.services.len());
                    local_service_to_idx.insert(trip.service_id.clone(), sidx);
                    self.timetable
                        .scoped_service_to_idx
                        .insert((feed_idx, trip.service_id.clone()), sidx);
                    self.timetable
                        .service_id_to_idx
                        .insert(trip.service_id.clone(), sidx);
                    self.timetable
                        .services
                        .push(ServiceCalendar::new(trip.service_id.clone()));
                    self.timetable.service_feed.push(feed_idx);
                    sidx
                }
            };

            let route_idx = local_route_to_idx[&trip.route_id];
            let shape_idx = trip
                .shape_id
                .as_ref()
                .and_then(|sid| local_shape_to_idx.get(sid).copied());

            let (place_indices, compact_sts) = match self.interpolate_and_pack_stop_times(
                &trip.id,
                &mut trip.stop_times,
                &local_stop_to_place,
                #[cfg(feature = "gtfs-flex")]
                &local_loc_to_place,
                #[cfg(feature = "gtfs-flex")]
                &local_grp_to_place,
            ) {
                Ok(res) => res,
                Err(err) => match self.validation_policy {
                    ValidationPolicy::Strict => return Err(err),
                    ValidationPolicy::DropInvalidTripsWithWarning => {
                        feed.dropped_counts.malformed_trips += 1;
                        feed.warnings.push(ValidationWarning::DroppedTrip {
                            trip_id: trip.id.to_string(),
                            reason: err.to_string(),
                        });
                        continue;
                    }
                },
            };

            let key = PatternKey {
                feed_idx,
                route_idx,
                places: place_indices.clone(),
                pickup_methods: trip
                    .stop_times
                    .iter()
                    .map(|s| pickup_code(&s.pickup))
                    .collect(),
                drop_off_methods: trip
                    .stop_times
                    .iter()
                    .map(|s| pickup_code(&s.drop_off))
                    .collect(),
            };

            let pattern_idx = match self.pattern_lookup.get(&key).copied() {
                Some(pidx) => pidx,
                None => {
                    let pidx = PatternIdx::from_usize(self.timetable.patterns.len());
                    let stops_profile: Vec<PatternStopProfile> = trip
                        .stop_times
                        .iter()
                        .zip(place_indices.iter())
                        .map(|(st, &place_idx)| PatternStopProfile {
                            place_idx,
                            pickup: st.pickup.clone(),
                            drop_off: st.drop_off.clone(),
                        })
                        .collect();

                    for (offset, prof) in stops_profile.iter().enumerate() {
                        let occ = PlacePatternOccurrence {
                            pattern_idx: pidx,
                            stop_offset_in_pattern: offset as u16,
                        };
                        self.timetable.place_to_patterns[prof.place_idx.index()].push(occ);
                        let member_places =
                            self.timetable.group_to_member_places[prof.place_idx.index()].clone();
                        for member_pidx in member_places {
                            self.timetable.place_to_patterns[member_pidx.index()].push(occ);
                        }
                    }

                    self.timetable.route_to_patterns[route_idx.index()].push(pidx);
                    self.timetable.patterns.push(RoutePattern {
                        idx: pidx,
                        feed_idx,
                        route_idx,
                        stops: stops_profile,
                        trips: Vec::new(),
                    });
                    self.pattern_lookup.insert(key, pidx);
                    pidx
                }
            };

            if self.materialize_frequency_trips
                && !trip.frequencies.is_empty()
                && !compact_sts.is_empty()
                && compact_sts.iter().all(|st| !st.is_flex_window())
            {
                let t0_secs = compact_sts[0].departure_or_end_secs as i64;
                let mut instance_counter = 0usize;
                for freq in &trip.frequencies {
                    if freq.headway_secs == 0 {
                        continue;
                    }
                    let mut dep0 = freq.window.start.0 as i64;
                    let end0 = freq.window.end.0 as i64;
                    let headway = freq.headway_secs as i64;
                    while dep0 < end0 || (instance_counter == 0 && dep0 == end0) {
                        let offset_secs = dep0 - t0_secs;
                        let shifted_sts: Vec<CompactStopTime> = compact_sts
                            .iter()
                            .map(|st| CompactStopTime {
                                arrival_or_start_secs: GtfsTime(st.arrival_or_start_secs)
                                    .add_signed_seconds(offset_secs)
                                    .0,
                                departure_or_end_secs: GtfsTime(st.departure_or_end_secs)
                                    .add_signed_seconds(offset_secs)
                                    .0,
                                ..*st
                            })
                            .collect();

                        let stop_times_offset = self.timetable.stop_times.len() as u32;
                        let stop_count = shifted_sts.len() as u16;
                        self.timetable.stop_times.extend(shifted_sts);

                        let trip_idx = TripIdx::from_usize(self.timetable.trips.len());
                        let expanded_id = if instance_counter == 0 {
                            trip.id.clone()
                        } else {
                            TripId::new(format!("{}#freq_{}", trip.id, instance_counter))
                        };
                        self.timetable
                            .scoped_trip_to_idx
                            .insert((feed_idx, expanded_id.clone()), trip_idx);
                        self.timetable
                            .trip_id_to_idx
                            .insert(expanded_id.clone(), trip_idx);
                        self.timetable.patterns[pattern_idx.index()]
                            .trips
                            .push(trip_idx);

                        self.timetable.trips.push(RoutableTrip {
                            idx: trip_idx,
                            feed_idx,
                            id: expanded_id,
                            route_idx,
                            pattern_idx,
                            service_idx,
                            shape_idx,
                            headsign: trip.trip_headsign.clone(),
                            stop_times_offset,
                            stop_count,
                            frequencies: Vec::new(),
                        });

                        instance_counter += 1;
                        dep0 += headway;
                    }
                }
            } else {
                let stop_times_offset = self.timetable.stop_times.len() as u32;
                let stop_count = compact_sts.len() as u16;
                self.timetable.stop_times.extend(compact_sts);

                let trip_idx = TripIdx::from_usize(self.timetable.trips.len());
                self.timetable
                    .scoped_trip_to_idx
                    .insert((feed_idx, trip.id.clone()), trip_idx);
                self.timetable
                    .trip_id_to_idx
                    .insert(trip.id.clone(), trip_idx);
                self.timetable.patterns[pattern_idx.index()]
                    .trips
                    .push(trip_idx);

                self.timetable.trips.push(RoutableTrip {
                    idx: trip_idx,
                    feed_idx,
                    id: trip.id,
                    route_idx,
                    pattern_idx,
                    service_idx,
                    shape_idx,
                    headsign: trip.trip_headsign,
                    stop_times_offset,
                    stop_count,
                    frequencies: trip.frequencies,
                });
            }
        }

        let patterns_end = self.timetable.patterns.len() as u32;
        let trips_end = self.timetable.trips.len() as u32;
        let services_end = self.timetable.services.len() as u32;

        // Sort trips within each newly created pattern by earliest time at the first stop
        for pat_u32 in patterns_start..patterns_end {
            let pattern = &mut self.timetable.patterns[pat_u32 as usize];
            let trips_ref = &self.timetable.trips;
            let st_arena = &self.timetable.stop_times;
            pattern.trips.sort_by_key(|&tidx| {
                let t = &trips_ref[tidx.index()];
                if t.stop_count > 0 {
                    st_arena[t.stop_times_offset as usize].arrival_or_start_secs
                } else {
                    0
                }
            });
        }

        let primary_timezone = feed
            .agencies
            .iter()
            .find_map(|a| a.parsed_timezone.clone());

        self.timetable.feeds.push(FeedMetadata {
            idx: feed_idx,
            id: feed_id,
            agencies: feed.agencies,
            primary_timezone,
            places_span: places_start..places_end,
            routes_span: routes_start..routes_end,
            patterns_span: patterns_start..patterns_end,
            trips_span: trips_start..trips_end,
            services_span: services_start..services_end,
            shapes_span: shapes_start..shapes_end,
            warnings: feed.warnings,
            dropped_counts: feed.dropped_counts,
        });

        Ok(feed_idx)
    }

    /// Consumes the builder, bulk-loads the [`rstar::RTree`] spatial index over all places,
    /// and returns the completed [`RoutingTimetable`].
    pub fn build(mut self) -> RoutingTimetable {
        self.timetable.stop_times.shrink_to_fit();
        self.timetable.places.shrink_to_fit();
        self.timetable.trips.shrink_to_fit();
        self.timetable.patterns.shrink_to_fit();
        self.timetable.place_spatial_index = build_place_spatial_index(&self.timetable.places);
        self.timetable
    }

    #[cfg(feature = "gtfs-flex")]
    fn intern_flex_duration(&mut self, params: FlexDurationParams) -> u16 {
        if !params.is_some() {
            return NO_FLEX_DURATION_IDX;
        }
        let key = [
            params.mean_duration_factor.unwrap_or(f64::NAN).to_bits(),
            params.mean_duration_offset.unwrap_or(f64::NAN).to_bits(),
            params.safe_duration_factor.unwrap_or(f64::NAN).to_bits(),
            params.safe_duration_offset.unwrap_or(f64::NAN).to_bits(),
        ];
        if let Some(&idx) = self.flex_duration_lookup.get(&key) {
            return idx;
        }
        let idx = self
            .timetable
            .flex_duration_table
            .len()
            .min(u16::MAX as usize - 1) as u16;
        self.timetable.flex_duration_table.push(params);
        self.flex_duration_lookup.insert(key, idx);
        idx
    }

    fn interpolate_and_pack_stop_times(
        &mut self,
        trip_id: &TripId,
        stop_times: &mut [StopTime],
        stop_id_to_place: &HashMap<StopId, PlaceIdx>,
        #[cfg(feature = "gtfs-flex")] location_id_to_place: &HashMap<LocationId, PlaceIdx>,
        #[cfg(feature = "gtfs-flex")] location_group_id_to_place: &HashMap<
            LocationGroupId,
            PlaceIdx,
        >,
    ) -> Result<(Vec<PlaceIdx>, Vec<CompactStopTime>), RoutingError> {
        let n = stop_times.len();
        if n == 0 {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut fixed_times: Vec<Option<(GtfsTime, GtfsTime)>> = vec![None; n];
        for (i, st) in stop_times.iter().enumerate() {
            match st.timing {
                StopTimeTiming::Scheduled { arrival, departure } => {
                    fixed_times[i] = Some((arrival, departure));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: Some(a),
                    departure: None,
                } => {
                    fixed_times[i] = Some((a, a));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: None,
                    departure: Some(d),
                } => {
                    fixed_times[i] = Some((d, d));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: Some(a),
                    departure: Some(d),
                } => {
                    fixed_times[i] = Some((a, d));
                }
                StopTimeTiming::Uninterpolated {
                    arrival: None,
                    departure: None,
                } => {}
                #[cfg(feature = "gtfs-flex")]
                StopTimeTiming::FlexWindow(_) => {}
            }
        }

        let mut i = 0;
        while i < n {
            if matches!(
                stop_times[i].timing,
                StopTimeTiming::Uninterpolated {
                    arrival: None,
                    departure: None
                }
            ) {
                if i == 0 {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[0].stop_sequence,
                    });
                }
                let prev_idx = i - 1;
                let Some((_, start_dep)) = fixed_times[prev_idx] else {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[i].stop_sequence,
                    });
                };

                let mut next_idx = i + 1;
                while next_idx < n
                    && matches!(
                        stop_times[next_idx].timing,
                        StopTimeTiming::Uninterpolated {
                            arrival: None,
                            departure: None
                        }
                    )
                {
                    next_idx += 1;
                }
                if next_idx >= n {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[n - 1].stop_sequence,
                    });
                }
                let Some((end_arr, _)) = fixed_times[next_idx] else {
                    return Err(RoutingError::UninterpolatableTripBoundary {
                        trip_id: trip_id.to_string(),
                        stop_sequence: stop_times[next_idx].stop_sequence,
                    });
                };

                let start_arc = stop_times[prev_idx].shape_ref.map(|r| r.arc_meters);
                let end_arc = stop_times[next_idx].shape_ref.map(|r| r.arc_meters);

                for k in i..next_idx {
                    let ratio = match (
                        start_arc,
                        end_arc,
                        stop_times[k].shape_ref.map(|r| r.arc_meters),
                    ) {
                        (Some(s_m), Some(e_m), Some(k_m)) if e_m > s_m => (k_m - s_m) / (e_m - s_m),
                        _ => (k - prev_idx) as f64 / (next_idx - prev_idx) as f64,
                    };
                    let interp_t = GtfsTime::lerp(start_dep, end_arr, ratio);
                    fixed_times[k] = Some((interp_t, interp_t));
                }
                i = next_idx;
            } else {
                i += 1;
            }
        }

        let mut prev_time: Option<GtfsTime> = None;
        let mut place_indices = Vec::with_capacity(n);
        let mut compact_sts = Vec::with_capacity(n);

        for (idx, st) in stop_times.iter().enumerate() {
            let place_idx = resolve_place_idx(
                &st.place,
                stop_id_to_place,
                #[cfg(feature = "gtfs-flex")]
                location_id_to_place,
                #[cfg(feature = "gtfs-flex")]
                location_group_id_to_place,
            );
            place_indices.push(place_idx);

            let shape_arc_meters = st
                .shape_ref
                .map(|r| r.arc_meters as f32)
                .unwrap_or(f32::NAN);

            #[cfg(feature = "gtfs-flex")]
            let flex_duration_idx = self.intern_flex_duration(st.flex_duration);
            #[cfg(not(feature = "gtfs-flex"))]
            let flex_duration_idx = NO_FLEX_DURATION_IDX;

            let compact = match st.timing {
                #[cfg(feature = "gtfs-flex")]
                StopTimeTiming::FlexWindow(w) => {
                    CompactStopTime::new_flex_window(w, shape_arc_meters, flex_duration_idx)
                }
                _ => {
                    let (arr, dep) =
                        fixed_times[idx].expect("all non-flex stop times resolved above");
                    if let Some(prev) = prev_time {
                        if arr < prev || dep < arr {
                            return Err(RoutingError::DecreasingStopTimes {
                                trip_id: trip_id.to_string(),
                                stop_sequence: st.stop_sequence,
                            });
                        }
                    }
                    prev_time = Some(dep);
                    CompactStopTime::new_fixed(arr, dep, shape_arc_meters, flex_duration_idx)
                }
            };

            compact_sts.push(compact);
        }

        Ok((place_indices, compact_sts))
    }
}

impl TryFrom<ValidatedFeed> for RoutingTimetable {
    type Error = RoutingError;

    fn try_from(feed: ValidatedFeed) -> Result<Self, Self::Error> {
        let mut builder = RoutingTimetableBuilder::new();
        builder.add_feed(FeedId::new("default"), feed)?;
        Ok(builder.build())
    }
}

fn resolve_place_idx(
    place: &StopPlace,
    stop_id_to_place: &HashMap<StopId, PlaceIdx>,
    #[cfg(feature = "gtfs-flex")] location_id_to_place: &HashMap<LocationId, PlaceIdx>,
    #[cfg(feature = "gtfs-flex")] location_group_id_to_place: &HashMap<LocationGroupId, PlaceIdx>,
) -> PlaceIdx {
    match place {
        StopPlace::Stop(s) => stop_id_to_place[&s.id],
        #[cfg(feature = "gtfs-flex")]
        StopPlace::FlexZone(z) => location_id_to_place[&z.id],
        #[cfg(feature = "gtfs-flex")]
        StopPlace::LocationGroup(g) => location_group_id_to_place[&g.id],
    }
}
