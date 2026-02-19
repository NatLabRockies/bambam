use std::ops::Add;

use chrono::{Duration, Months, NaiveDateTime, TimeDelta};
use skiplist::OrderedSkipList;

/// a schedule contains an ordered list of [`Departure`] values.
pub type Schedule = OrderedSkipList<Departure>;

/// a single departure from a src location, recorded as its pair of
/// departure time from here and arrival time at some dst location.
#[derive(Debug, Clone, Eq, Copy)]
pub struct Departure {
    pub src_departure_time: NaiveDateTime,
    pub dst_arrival_time: NaiveDateTime,
}

impl Departure {
    pub fn construct_query(datetime: NaiveDateTime) -> Self {
        Self {
            src_departure_time: datetime,
            dst_arrival_time: datetime,
        }
    }

    /// represent infinity in the time space of departures
    pub fn infinity() -> Self {
        Departure {
            src_departure_time: NaiveDateTime::MAX,
            dst_arrival_time: NaiveDateTime::MAX,
        }
    }

    /// the departure is placed at positive infinity. occurs
    /// when adding extreme TimeDelta values.
    pub fn is_pos_infinity(&self) -> bool {
        self.src_departure_time == NaiveDateTime::MAX || self.dst_arrival_time == NaiveDateTime::MAX
    }

    /// the departure is placed at negative infinity. occurs
    /// when adding extreme TimeDelta values.
    pub fn is_neg_infinity(&self) -> bool {
        self.src_departure_time == NaiveDateTime::MIN || self.dst_arrival_time == NaiveDateTime::MIN
    }
}

impl Add<&TimeDelta> for Departure {
    type Output = Departure;
    /// adds to a Departure. clamps at absolute MIN or MAX time values.
    fn add(self, rhs: &TimeDelta) -> Self::Output {
        let src_departure_time = add_time_to_datetime(&self.src_departure_time, rhs);
        let dst_arrival_time = add_time_to_datetime(&self.dst_arrival_time, rhs);
        Departure {
            src_departure_time,
            dst_arrival_time,
        }
    }
}

impl PartialEq for Departure {
    fn eq(&self, other: &Self) -> bool {
        self.src_departure_time == other.src_departure_time
            && self.dst_arrival_time == other.dst_arrival_time
    }
}

impl PartialOrd for Departure {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.src_departure_time
            .partial_cmp(&other.src_departure_time)
    }
}

impl Ord for Departure {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.src_departure_time.cmp(&other.src_departure_time)
    }
}

/// Adds a time delta to a datetime, clamping to MIN/MAX on overflow.
///
/// # Arguments
/// * `date_time` - The base datetime
/// * `time_delta` - The duration to add (can be negative)
///
/// # Returns
/// - The sum if it fits within NaiveDateTime's range
/// - NaiveDateTime::MIN if negative overflow occurs
/// - NaiveDateTime::MAX if positive overflow occurs
fn add_time_to_datetime(date_time: &NaiveDateTime, time_delta: &TimeDelta) -> NaiveDateTime {
    date_time
        .checked_add_signed(*time_delta)
        .unwrap_or_else(|| {
            if time_delta < &TimeDelta::zero() {
                NaiveDateTime::MIN
            } else {
                NaiveDateTime::MAX
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    #[test]
    fn test_departure_add_normal() {
        let departure = Departure {
            src_departure_time: NaiveDateTime::parse_from_str(
                "2023-06-15 10:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            dst_arrival_time: NaiveDateTime::parse_from_str(
                "2023-06-15 11:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
        };
        let delta = TimeDelta::hours(2);
        let result = departure + &delta;

        assert_eq!(
            result.src_departure_time,
            NaiveDateTime::parse_from_str("2023-06-15 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            result.dst_arrival_time,
            NaiveDateTime::parse_from_str("2023-06-15 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
    }

    #[test]
    fn test_departure_add_negative() {
        let departure = Departure {
            src_departure_time: NaiveDateTime::parse_from_str(
                "2023-06-15 10:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            dst_arrival_time: NaiveDateTime::parse_from_str(
                "2023-06-15 11:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
        };
        let delta = TimeDelta::hours(-2);
        let result = departure + &delta;

        assert_eq!(
            result.src_departure_time,
            NaiveDateTime::parse_from_str("2023-06-15 08:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            result.dst_arrival_time,
            NaiveDateTime::parse_from_str("2023-06-15 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
    }

    #[test]
    fn test_departure_add_overflow_to_max() {
        let departure = Departure {
            src_departure_time: NaiveDateTime::parse_from_str(
                "9999-12-31 23:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            dst_arrival_time: NaiveDateTime::parse_from_str(
                "9999-12-31 23:30:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
        };
        // Adding a huge duration that will overflow
        let delta = TimeDelta::days(365 * 1000000); // 1M years
        let result = departure + &delta;

        assert_eq!(
            result.src_departure_time,
            NaiveDateTime::MAX,
            "Should clamp to MAX on positive overflow"
        );
        assert_eq!(
            result.dst_arrival_time,
            NaiveDateTime::MAX,
            "Should clamp to MAX on positive overflow"
        );
    }

    #[test]
    fn test_departure_add_underflow_to_min() {
        let departure = Departure {
            src_departure_time: NaiveDateTime::parse_from_str(
                "0001-01-01 01:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            dst_arrival_time: NaiveDateTime::parse_from_str(
                "0001-01-01 01:30:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
        };
        // Subtracting a huge duration that will underflow
        let delta = TimeDelta::days(-365 * 1000000); // -1M years
        let result = departure + &delta;

        assert_eq!(
            result.src_departure_time,
            NaiveDateTime::MIN,
            "Should clamp to MIN on negative overflow"
        );
        assert_eq!(
            result.dst_arrival_time,
            NaiveDateTime::MIN,
            "Should clamp to MIN on negative overflow"
        );
    }

    #[test]
    fn test_departure_infinity() {
        let inf = Departure::infinity();
        assert!(inf.is_pos_infinity());
        assert_eq!(inf.src_departure_time, NaiveDateTime::MAX);
        assert_eq!(inf.dst_arrival_time, NaiveDateTime::MAX);
    }

    #[test]
    fn test_departure_add_to_infinity_stays_infinity() {
        let inf = Departure::infinity();
        let delta = TimeDelta::hours(5);
        let result = inf + &delta;

        // Adding to MAX should stay at MAX
        assert_eq!(result.src_departure_time, NaiveDateTime::MAX);
        assert_eq!(result.dst_arrival_time, NaiveDateTime::MAX);
        assert!(result.is_pos_infinity());
    }

    #[test]
    fn test_departure_ordering() {
        let early = Departure {
            src_departure_time: NaiveDateTime::parse_from_str(
                "2023-06-15 10:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            dst_arrival_time: NaiveDateTime::parse_from_str(
                "2023-06-15 11:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
        };
        let late = Departure {
            src_departure_time: NaiveDateTime::parse_from_str(
                "2023-06-15 12:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            dst_arrival_time: NaiveDateTime::parse_from_str(
                "2023-06-15 13:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
        };

        assert!(early < late);
        assert!(late > early);
        assert_eq!(early, early);
    }
}
