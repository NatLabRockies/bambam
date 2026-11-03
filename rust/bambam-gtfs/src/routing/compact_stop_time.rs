//! Ultra-compact 16-byte `CompactStopTime` and `RoutableStopTiming` for the Phase 3 CSR arena.

use crate::primitives::{GtfsTime, TimeWindow};
use crate::traits::TemporalStopTime;

/// Interpolated or flexible timing at a single stop event in the Phase 3 [`crate::routing::RoutingTimetable`].
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
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

pub(crate) const FLAG_IS_FLEX_WINDOW: u16 = 1 << 0;
pub(crate) const NO_FLEX_DURATION_IDX: u16 = u16::MAX;

/// Ultra-compact `16-byte` stop-time entry stored in the flat CSR [`crate::routing::RoutingTimetable::stop_times`] arena.
///
/// Why 16 bytes?
/// - Across 1,000 US GTFS feeds (~200 million stop_times), a 16-byte record consumes only **3.2 GB**
///   total (compared to ~24 GB for uncompacted structs with inline `Option<ShapeLinearRef>` and
///   `FlexDurationParams`), and **4 consecutive stop times fit inside a single 64-byte CPU cache line**.
/// - Because every trip belonging to a [`crate::routing::RoutePattern`] visits the exact same sequence of places,
///   storing `place_idx` on every stop time is redundant with `pattern.stops[offset].place_idx`.
#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CompactStopTime {
    /// Scheduled/interpolated arrival time in seconds since midnight, or flex window `start` when `FLAG_IS_FLEX_WINDOW` is set.
    pub arrival_or_start_secs: u32,
    /// Scheduled/interpolated departure time in seconds since midnight, or flex window `end` when `FLAG_IS_FLEX_WINDOW` is set.
    pub departure_or_end_secs: u32,
    /// Cumulative Haversine arc-length offset in meters along the trip's [`crate::model::ShapePolyline`], or `f32::NAN` if unavailable.
    pub shape_arc_meters: f32,
    /// Index into `RoutingTimetable::flex_duration_table`, or `u16::MAX` if no GTFS-Flex duration parameters apply.
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
