use chrono::NaiveDateTime;
use routee_compass::plugin::input::default;
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
