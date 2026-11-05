use bambam_gtfs::{EdgeDeparture, EdgeTimetableBuilder};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// guarantees we only log a dst < src data error ONCE
static SRC_DST_ORDERING_ERROR_LOCK: OnceLock<()> = OnceLock::new();

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
    /// Returns `true` if a departure with `(src_departure_time, dst_arrival_time)` is valid
    /// and within this policy's date range.
    pub fn accepts(
        &self,
        src_departure_time: NaiveDateTime,
        dst_arrival_time: NaiveDateTime,
    ) -> bool {
        if src_departure_time > dst_arrival_time {
            SRC_DST_ORDERING_ERROR_LOCK.get_or_init(|| {
                let src_time = src_departure_time.format("%Y-%m-%d %H:%M:%S");
                let dst_time = dst_arrival_time.format("%Y-%m-%d %H:%M:%S");
                let msg = format!("encountered at least one schedule row where src time {src_time} is not earlier than {dst_time}. the schedule may be corrupted.");
                log::warn!("{msg}");
            });
            return false;
        }

        match self {
            ScheduleLoadingPolicy::All => true,
            ScheduleLoadingPolicy::InDateRange {
                start_date,
                end_date,
            } => (src_departure_time <= *end_date) && (*start_date <= src_departure_time),
        }
    }

    pub fn insert_into_edge_timetable(
        &self,
        builder: &mut EdgeTimetableBuilder,
        edge_id: usize,
        route_label: i64,
        departure: EdgeDeparture,
    ) {
        builder.ensure_edge(edge_id);
        if self.accepts(departure.src_departure_time, departure.dst_arrival_time) {
            builder.add_departure(edge_id, route_label, departure);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bambam_gtfs::EdgeTimetable;
    use chrono::NaiveDate;

    #[test]
    fn test_invalid_midnight_wraparound_rejection() {
        let policy = ScheduleLoadingPolicy::All;
        let mut builder = EdgeTimetable::builder();

        // GTFS departure at 23:55, arriving incorrectly at unpadded 00:05 the same day
        let invalid_departure = EdgeDeparture::without_walk_distance(
            NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(23, 55, 0)
                .unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(0, 5, 0)
                .unwrap(),
        );

        policy.insert_into_edge_timetable(&mut builder, 0, 1, invalid_departure);
        let timetable = builder.build().unwrap();

        assert_eq!(
            timetable.departures.len(),
            0,
            "Schedule should reject departures where arrival happens before departure"
        );
    }
}
