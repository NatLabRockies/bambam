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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_window_validation_and_bounds() {
        let start = GtfsTime::from_hms(8, 0, 0);
        let end = GtfsTime::from_hms(9, 30, 0);

        let win = TimeWindow::new(start, end).expect("valid time window");
        assert_eq!(win.duration_seconds(), 5400);
        assert!(!win.contains(GtfsTime::from_hms(7, 59, 59)));
        assert!(win.contains(start));
        assert!(win.contains(GtfsTime::from_hms(8, 45, 0)));
        assert!(win.contains(end));
        assert!(!win.contains(GtfsTime::from_hms(9, 30, 1)));

        let zero_win = TimeWindow::new(start, start).expect("zero-duration window is valid");
        assert_eq!(zero_win.duration_seconds(), 0);
        assert!(zero_win.contains(start));

        assert_eq!(
            TimeWindow::new(end, start),
            Err(PrimitiveError::InvalidTimeWindow {
                start: end,
                end: start
            })
        );
    }
}

