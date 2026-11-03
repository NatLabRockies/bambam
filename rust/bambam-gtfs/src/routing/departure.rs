//! Active service window (`ActiveServiceWindow`) and earliest boardable departure (`BoardableDeparture`).

use crate::ids::TripIdx;
use crate::primitives::GtfsTime;
use crate::routing::RoutingTimetable;
use crate::traits::TemporalStopTime;
use chrono::NaiveDate;

/// Active service evaluation window for a local `(date, time_of_day)` query, supporting both
/// current-day schedules and previous-day overnight trips (`>= 24:00:00` on `date - 1`).
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
pub struct ActiveServiceWindow {
    /// Local calendar date of the query.
    #[rkyv(with = crate::primitives::RkyvNaiveDate)]
    pub date: NaiveDate,
    /// Local time of day on `date` (`0..86_400` seconds).
    pub time_of_day: GtfsTime,
    /// Boolean mask indexed by [`crate::ids::ServiceIdx`] indicating services active on `date`.
    pub today_active: Vec<bool>,
    /// Previous calendar date (`date - 1 day`).
    #[rkyv(with = crate::primitives::RkyvNaiveDate)]
    pub yesterday_date: NaiveDate,
    /// Query time expressed on `yesterday_date`'s service clock (`time_of_day + 86_400` seconds).
    pub yesterday_overnight_time: GtfsTime,
    /// Boolean mask indexed by [`crate::ids::ServiceIdx`] indicating services active on `yesterday_date`.
    pub yesterday_active: Vec<bool>,
}

/// Earliest boardable trip or frequency/flex departure returned by
/// [`RoutingTimetable::earliest_boardable_trip`] or [`RoutingTimetable::earliest_boardable_trip_with_overnight`].
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
pub struct BoardableDeparture {
    /// Contiguous index of the matched [`crate::routing::RoutableTrip`].
    pub trip_idx: TripIdx,
    /// Earliest time the rider can board at `stop_offset` (in the service day's `GtfsTime` clock).
    pub departure_time: GtfsTime,
    /// Signed offset in seconds relative to the template trip's stop times:
    /// - `0` for fixed-schedule trips, materialized frequency trips, or flex windows,
    /// - `> 0` (or `< 0`) when boarding a non-materialized `frequencies.txt` headway departure.
    pub frequency_offset_secs: i64,
    /// `true` if this departure belongs to yesterday's service day (`date - 1` with GTFS time `>= 24:00:00`).
    pub is_overnight_from_yesterday: bool,
}

impl BoardableDeparture {
    /// Computes the arrival time at a downstream `to_stop_offset` along the same trip,
    /// automatically applying `self.frequency_offset_secs`.
    pub fn arrival_time_at(
        &self,
        timetable: &RoutingTimetable,
        to_stop_offset: usize,
    ) -> Option<GtfsTime> {
        let st = timetable.trip_stop_times(self.trip_idx).get(to_stop_offset)?;
        let base = st.earliest_time()?;
        Some(base.add_signed_seconds(self.frequency_offset_secs))
    }

    /// Normalizes `self.departure_time` back to the current calendar day's clock
    /// (subtracting `86_400` seconds when `self.is_overnight_from_yesterday` is `true`).
    #[inline]
    pub const fn normalized_query_day_departure(&self) -> GtfsTime {
        if self.is_overnight_from_yesterday {
            GtfsTime(self.departure_time.0.saturating_sub(86_400))
        } else {
            self.departure_time
        }
    }
}
