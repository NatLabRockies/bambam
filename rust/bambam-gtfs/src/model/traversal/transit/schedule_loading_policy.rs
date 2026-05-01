use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::model::traversal::transit::schedule::{Departure, Schedule};

#[derive(Debug, Default, Serialize, Deserialize)]
pub enum ScheduleLoadingPolicy {
    #[default]
    All,
    InDateRange {
        start_date: NaiveDateTime,
        end_date: NaiveDateTime,
    },
}

impl ScheduleLoadingPolicy {
    pub fn insert_if_valid(&self, schedule_skiplist: &mut Schedule, element: Departure) {
        if element.src_departure_time > element.dst_arrival_time {
            return;
        }

        let should_insert = match self {
            ScheduleLoadingPolicy::All => true,
            ScheduleLoadingPolicy::InDateRange {
                start_date,
                end_date,
            } => {
                (element.src_departure_time <= *end_date)
                    && (*start_date <= element.src_departure_time)
            }
        };

        if should_insert {
            schedule_skiplist.insert(element);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_invalid_midnight_wraparound_rejection() {
        let policy = ScheduleLoadingPolicy::All;
        let mut schedule = Schedule::new();

        // GTFS departure at 23:55, arriving incorrectly at unpadded 00:05 the same day
        let invalid_departure = Departure {
            src_departure_time: NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(23, 55, 0)
                .unwrap(),
            dst_arrival_time: NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(0, 5, 0)
                .unwrap(),
        };

        policy.insert_if_valid(&mut schedule, invalid_departure);

        // Without the bug fix restricting src <= dst, this will fail if we assert it should be empty
        // The expected behavior once fixed is for the schedule to remain length 0
        assert_eq!(
            schedule.len(),
            0,
            "Schedule should reject departures where arrival happens before departure"
        );
    }
}
