//! Route pattern (`RoutePattern`), stop boarding profile (`PatternStopProfile`),
//! reverse pattern occurrence (`PlacePatternOccurrence`), and indexed transfer (`IndexedTransfer`).

use crate::enums::TransferType;
use crate::ids::*;
use crate::model::BoardingPolicy;

/// Stop-level boarding and alighting profile entry within a [`RoutePattern`].
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PatternStopProfile {
    /// Contiguous index of the visited place (`Stop`, `FlexZone`, or `LocationGroup`).
    pub place_idx: PlaceIdx,
    /// Pickup policy at this stop along the pattern.
    pub pickup: BoardingPolicy,
    /// Drop-off policy at this stop along the pattern.
    pub drop_off: BoardingPolicy,
}

/// A unique sequence of [`PatternStopProfile`]s served by one or more trips on a [`crate::model::Route`].
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
pub struct RoutePattern {
    /// Contiguous index of this pattern.
    pub idx: PatternIdx,
    /// Contiguous index of the archive/feed that owns this pattern.
    pub feed_idx: FeedIdx,
    /// Contiguous index of the parent [`crate::model::Route`].
    pub route_idx: RouteIdx,
    /// Ordered stop sequence and boarding/alighting policies for this pattern.
    pub stops: Vec<PatternStopProfile>,
    /// Contiguous [`TripIdx`]s belonging to this pattern, sorted by earliest departure at stop 0.
    pub trips: Vec<TripIdx>,
}

/// Reverse-index entry mapping a [`PlaceIdx`] to a [`PatternIdx`] and its 0-based position
/// within `pattern.stops`.
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
pub struct PlacePatternOccurrence {
    /// Pattern visiting the place.
    pub pattern_idx: PatternIdx,
    /// 0-based offset into `pattern.stops` (and `timetable.trip_stop_times(trip_idx)`).
    pub stop_offset_in_pattern: u16,
}

/// Pre-indexed stop-to-stop transfer or pathway edge between two [`PlaceIdx`]s.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
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
