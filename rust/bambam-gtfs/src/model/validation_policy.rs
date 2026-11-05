//! Validation policy (`ValidationPolicy`), structured warnings (`ValidationWarning`), and dropped entity summary counts (`DroppedEntityCounts`).

/// Policy governing how Phase 2 ([`crate::model::ValidatedFeed`]) and Phase 3 ([`crate::routing::RaptorTimetableBuilder`])
/// handle real-world "dirty GTFS" minor specification violations (such as trips with 0 or 1 stops,
/// non-monotonic stop timestamps, missing boundary timestamps, duplicate `stop_sequence` numbers,
/// or broken foreign keys on individual trips/rows).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum ValidationPolicy {
    /// Fail immediately on the first relational or timing violation.
    #[default]
    Strict,
    /// Drop malformed trips, stop times, or dangling secondary rows while recording
    /// structured [`ValidationWarning`]s and [`DroppedEntityCounts`].
    DropInvalidTripsWithWarning,
}

/// Structured warning emitted when a GTFS entity or row is dropped or repaired under
/// [`ValidationPolicy::DropInvalidTripsWithWarning`].
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
pub enum ValidationWarning {
    /// A trip was dropped during Phase 2 or Phase 3 validation.
    DroppedTrip {
        /// Identifier of the dropped trip.
        trip_id: String,
        /// Reason the trip was dropped.
        reason: String,
    },
    /// Duplicate `stop_sequence` numbers were found on a trip and deduplicated.
    DuplicateStopSequence {
        /// Trip identifier.
        trip_id: String,
        /// Duplicate `stop_sequence` value.
        stop_sequence: u32,
    },
    /// An orphan `stop_times.txt` row referencing a non-existent `trip_id` was skipped.
    DroppedOrphanStopTime {
        /// Missing `trip_id`.
        trip_id: String,
        /// `stop_sequence` on the orphan row.
        stop_sequence: u32,
    },
    /// A secondary entity row (`transfers.txt`, `pathways.txt`, `frequencies.txt`, `shapes.txt`, etc.)
    /// was dropped due to a broken reference or invalid domain value.
    DroppedEntity {
        /// Source file name.
        file_name: String,
        /// Identifier of the dropped row/entity.
        id: String,
        /// Reason for dropping.
        reason: String,
    },
}

/// Summary counts of dropped or deduplicated GTFS entities when loading with
/// [`ValidationPolicy::DropInvalidTripsWithWarning`].
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct DroppedEntityCounts {
    /// Number of trips dropped because they had 0 stops in `stop_times.txt`.
    pub empty_trips: usize,
    /// Number of trips dropped because they had only 1 stop in `stop_times.txt`.
    pub single_stop_trips: usize,
    /// Number of trips dropped due to invalid timing, decreasing stop times, missing boundary times,
    /// non-monotonic shape distances, or unresolvable stop places.
    pub malformed_trips: usize,
    /// Number of duplicate `stop_sequence` rows deduplicated within trips.
    pub duplicate_stop_sequences: usize,
    /// Number of orphan `stop_times.txt` rows referencing unknown `trip_id`s.
    pub orphan_stop_times: usize,
    /// Number of invalid `shapes.txt`, `frequencies.txt`, `transfers.txt`, or `pathways.txt` rows dropped.
    pub dropped_secondary_rows: usize,
}

impl DroppedEntityCounts {
    /// Total number of trips dropped across all categories.
    pub const fn total_dropped_trips(&self) -> usize {
        self.empty_trips + self.single_stop_trips + self.malformed_trips
    }
}
