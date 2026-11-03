//! Inclusive service-day time interval (`TimeWindow`).

use crate::primitives::{GtfsTime, PrimitiveError};

/// An inclusive time interval `[start, end]` on a service day (used for GTFS-Flex pickup/drop-off windows
/// and frequency windows).
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
pub struct TimeWindow {
    /// Start of the window (inclusive).
    pub start: GtfsTime,
    /// End of the window (inclusive).
    pub end: GtfsTime,
}

impl TimeWindow {
    /// Creates a validated `TimeWindow` where `start <= end`.
    pub fn new(start: GtfsTime, end: GtfsTime) -> Result<Self, PrimitiveError> {
        if start > end {
            return Err(PrimitiveError::InvalidTimeWindow { start, end });
        }
        Ok(Self { start, end })
    }

    /// Returns the duration of the window in seconds.
    #[inline]
    pub const fn duration_seconds(&self) -> u32 {
        self.end.0.saturating_sub(self.start.0)
    }

    /// Returns true if `time` falls within `[self.start, self.end]`.
    #[inline]
    pub const fn contains(&self, time: GtfsTime) -> bool {
        time.0 >= self.start.0 && time.0 <= self.end.0
    }
}
