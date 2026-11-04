//! Phase 2 (`model`) relational and domain validation errors.

use crate::primitives::PrimitiveError;
use thiserror::Error;

/// Errors occurring during Phase 2 when transforming a [`RawFeed`](crate::raw::RawFeed)
/// into a relationally validated [`ValidatedFeed`](crate::model::ValidatedFeed).
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ModelError {
    /// Invalid primitive value encountered during model construction (e.g. inverted time window).
    #[error(transparent)]
    Primitive(#[from] PrimitiveError),

    /// A foreign key references an entity ID that does not exist in the target table.
    #[error(
        "missing reference in '{source_file}': {field_name} '{id}' not found in {target_file}"
    )]
    MissingReference {
        /// File containing the broken reference (e.g. `stop_times.txt`).
        source_file: &'static str,
        /// Column/field name holding the foreign key (e.g. `stop_id`).
        field_name: &'static str,
        /// The missing identifier value.
        id: String,
        /// Target file(s) where the ID was expected (e.g. `stops.txt`).
        target_file: &'static str,
    },

    /// An identifier appears in both `stops.txt` and `locations.geojson`, violating the GTFS-Flex shared namespace rule.
    #[error("namespace collision: ID '{id}' is defined in both stops.txt and locations.geojson")]
    NamespaceCollision {
        /// Conflicting identifier string.
        id: String,
    },

    /// A `stop_times.txt` row populates more than one of `stop_id`, `location_id`, or `location_group_id`.
    #[error("trip '{trip_id}' stop_sequence {stop_sequence} populates multiple mutually exclusive location fields ({fields})")]
    MutuallyExclusivePlaceFields {
        /// Trip identifier.
        trip_id: String,
        /// Stop sequence on the trip.
        stop_sequence: u32,
        /// Description of conflicting fields.
        fields: &'static str,
    },

    /// A `stop_times.txt` row does not reference any stop, location, or location group.
    #[error("trip '{trip_id}' stop_sequence {stop_sequence} is missing a stop_id, location_id, or location_group_id")]
    MissingStopTimePlace {
        /// Trip identifier.
        trip_id: String,
        /// Stop sequence on the trip.
        stop_sequence: u32,
    },

    /// A `stop_times.txt` row mixes fixed arrival/departure times with a GTFS-Flex pickup/drop-off window,
    /// or defines only one half of a flex window.
    #[error(
        "trip '{trip_id}' stop_sequence {stop_sequence} has invalid timing combination: {reason}"
    )]
    InvalidStopTimeTiming {
        /// Trip identifier.
        trip_id: String,
        /// Stop sequence on the trip.
        stop_sequence: u32,
        /// Explanation of the timing violation.
        reason: String,
    },

    /// Non-monotonic `shape_dist_traveled` values in `shapes.txt` or `stop_times.txt`.
    #[error("non-monotonic shape_dist_traveled in '{file_name}' for '{id}': {previous} followed by {current}")]
    NonMonotonicShapeDistance {
        /// File where non-monotonic distance occurred.
        file_name: &'static str,
        /// Shape ID or Trip ID.
        id: String,
        /// Previous distance value.
        previous: f64,
        /// Current smaller distance value.
        current: f64,
    },
}
