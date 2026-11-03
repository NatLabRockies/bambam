//! Primitive parsing and validation errors.

use crate::primitives::GtfsTime;
use thiserror::Error;

/// Errors that can occur when parsing primitive GTFS values (times, dates, colors, floats).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveError {
    /// A time string is not in valid `HH:MM:SS` format.
    #[error("'{0}' is not a valid GTFS time; expected HH:MM:SS")]
    InvalidTime(String),
    /// A time window has `start > end`.
    #[error("invalid time window: start ({start}) is after end ({end})")]
    InvalidTimeWindow {
        /// Window start time
        start: GtfsTime,
        /// Window end time
        end: GtfsTime,
    },
    /// A date string is not in valid `YYYYMMDD` format.
    #[error("'{0}' is not a valid GTFS date; expected YYYYMMDD")]
    InvalidDate(String),
    /// A color string is not in valid 6-digit hex `RRGGBB` format.
    #[error("'{0}' is not a valid hex color; expected RRGGBB without '#'")]
    InvalidColor(String),
}
