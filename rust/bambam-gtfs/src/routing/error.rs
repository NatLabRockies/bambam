//! Phase 3 (`routing`) errors when constructing the [`RoutingTimetable`](crate::routing::RoutingTimetable)
//! or querying trip sub-segment linestrings.

use crate::ids::TripIdx;
use thiserror::Error;

/// Errors occurring during Phase 3 (`RoutingTimetable` construction and routing queries).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum RoutingError {
    /// A trip has fixed/interpolated stop times at intermediate stops, but its first or last stop
    /// lacks a scheduled timestamp and cannot be anchored for linear interpolation.
    #[error("cannot interpolate stop times for trip '{trip_id}': missing boundary time at stop_sequence {stop_sequence}")]
    UninterpolatableTripBoundary {
        /// Trip identifier.
        trip_id: String,
        /// Stop sequence missing a boundary time.
        stop_sequence: u32,
    },

    /// Stop-time timestamps along a trip decrease after interpolation (`departure < previous_departure`).
    #[error("non-monotonic stop times in trip '{trip_id}' at stop_sequence {stop_sequence}")]
    DecreasingStopTimes {
        /// Trip identifier.
        trip_id: String,
        /// Stop sequence where time decreased.
        stop_sequence: u32,
    },

    /// Requested stop-offset range `[from_stop_offset, to_stop_offset]` is out of bounds for the trip.
    #[error("invalid stop offset range [{from_offset}..={to_offset}] for trip index {trip_idx} (trip has {num_stops} stops)")]
    InvalidStopOffsetRange {
        /// Contiguous trip index.
        trip_idx: TripIdx,
        /// Requested boarding offset along the trip pattern.
        from_offset: usize,
        /// Requested alighting offset along the trip pattern.
        to_offset: usize,
        /// Total number of stops in the trip's pattern.
        num_stops: usize,
    },
}
