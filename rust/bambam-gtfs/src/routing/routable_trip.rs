//! Validated, interpolated trip (`RoutableTrip`) in the [`crate::routing::RaptorTimetable`].

use crate::ids::*;
use crate::model::FrequencyWindow;
use std::ops::Range;

/// A validated, interpolated trip in the [`crate::routing::RaptorTimetable`] referencing a contiguous slice
/// of [`crate::routing::CompactStopTime`]s in `RaptorTimetable::stop_times`.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct RoutableTrip {
    /// Contiguous index of this trip.
    pub idx: TripIdx,
    /// Contiguous index of the archive/feed that owns this trip.
    pub feed_idx: FeedIdx,
    /// Original GTFS `TripId`.
    pub id: TripId,
    /// Contiguous index of the parent [`crate::model::Route`].
    pub route_idx: RouteIdx,
    /// Contiguous index of the [`crate::routing::RoutePattern`] this trip follows.
    pub pattern_idx: PatternIdx,
    /// Contiguous index of the [`crate::model::ServiceCalendar`] governing when this trip runs.
    pub service_idx: ServiceIdx,
    /// Optional contiguous index of the [`crate::model::ShapePolyline`] traced by this trip.
    pub shape_idx: Option<ShapeIdx>,
    /// Optional trip headsign.
    pub headsign: Option<String>,
    /// Start index into `RaptorTimetable::stop_times` where this trip's `stop_count` entries begin.
    pub stop_times_offset: u32,
    /// Number of stops on this trip (always equal to `pattern.stops.len()`).
    pub stop_count: u16,
    /// Optional frequency headway windows.
    pub frequencies: Vec<FrequencyWindow>,
}

impl RoutableTrip {
    /// Returns the index range into `RaptorTimetable::stop_times` for this trip.
    #[inline]
    pub const fn stop_times_range(&self) -> Range<usize> {
        let start = self.stop_times_offset as usize;
        start..(start + self.stop_count as usize)
    }
}
