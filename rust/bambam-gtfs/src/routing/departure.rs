//! Active service window (`ActiveServiceWindow`) and earliest boardable departure (`BoardableDeparture`).

use crate::ids::TripIdx;
use crate::primitives::GtfsTime;
use crate::routing::RoutingTimetable;
use crate::traits::TemporalStopTime;
use chrono::{DateTime, Days, NaiveDate};
use chrono_tz::Tz;

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
        let st = timetable
            .trip_stop_times(self.trip_idx)
            .get(to_stop_offset)?;
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

    /// Resolves this departure into a timezone-aware [`DateTime<Tz>`] in the trip's agency timezone,
    /// given the `query_date` of the [`ActiveServiceWindow`] (automatically accounting for
    /// `self.is_overnight_from_yesterday`).
    pub fn departure_datetime(
        &self,
        timetable: &RoutingTimetable,
        query_date: NaiveDate,
    ) -> Option<DateTime<Tz>> {
        let tz = timetable.trip_timezone(self.trip_idx)?;
        let service_date = if self.is_overnight_from_yesterday {
            query_date.checked_sub_days(Days::new(1))?
        } else {
            query_date
        };
        self.departure_time.to_zoned_datetime(service_date, tz)
    }

    /// Resolves the arrival at `to_stop_offset` along this trip into a timezone-aware [`DateTime<Tz>`]
    /// in the trip's agency timezone, given the `query_date` of the [`ActiveServiceWindow`].
    pub fn arrival_datetime_at(
        &self,
        timetable: &RoutingTimetable,
        query_date: NaiveDate,
        to_stop_offset: usize,
    ) -> Option<DateTime<Tz>> {
        let tz = timetable.trip_timezone(self.trip_idx)?;
        let arr_time = self.arrival_time_at(timetable, to_stop_offset)?;
        let service_date = if self.is_overnight_from_yesterday {
            query_date.checked_sub_days(Days::new(1))?
        } else {
            query_date
        };
        arr_time.to_zoned_datetime(service_date, tz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::*;
    use crate::model::ValidatedFeed;
    use crate::raw::{RawFeed, RawStopTime, RawTrip};
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn fixture_path(rel_path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path)
    }

    #[test]
    fn test_timezone_and_overnight_service_day_normalization() {
        let mut raw = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        // Add an overnight trip on service1 departing at 24:30:00 (00:30 AM next calendar day)
        raw.trips.push(RawTrip {
            id: TripId::new("overnight_trip"),
            service_id: ServiceId::new("service1"),
            route_id: RouteId::new("route1"),
            ..Default::default()
        });
        raw.stop_times.push(RawStopTime {
            trip_id: TripId::new("overnight_trip"),
            stop_id: Some(StopId::new("stop2")),
            stop_sequence: 1,
            arrival_time: Some(GtfsTime::from_hms(24, 30, 0)),
            departure_time: Some(GtfsTime::from_hms(24, 30, 0)),
            ..Default::default()
        });
        raw.stop_times.push(RawStopTime {
            trip_id: TripId::new("overnight_trip"),
            stop_id: Some(StopId::new("stop3")),
            stop_sequence: 2,
            arrival_time: Some(GtfsTime::from_hms(24, 40, 0)),
            departure_time: Some(GtfsTime::from_hms(24, 40, 0)),
            ..Default::default()
        });
        raw.stop_times.push(RawStopTime {
            trip_id: TripId::new("overnight_trip"),
            stop_id: Some(StopId::new("stop4")),
            stop_sequence: 3,
            arrival_time: Some(GtfsTime::from_hms(24, 50, 0)),
            departure_time: Some(GtfsTime::from_hms(24, 50, 0)),
            ..Default::default()
        });
        raw.frequencies = None;

        let validated = ValidatedFeed::try_from(raw).unwrap();
        assert_eq!(validated.agencies[0].timezone, chrono_tz::Tz::Europe__Paris);
        assert_eq!(
            validated.primary_timezone(),
            Some(chrono_tz::Tz::Europe__Paris)
        );

        let service_date = NaiveDate::from_ymd_opt(2017, 1, 8).unwrap();
        let zoned_overnight = validated
            .resolve_trip_datetime(
                "overnight_trip",
                service_date,
                GtfsTime::from_hms(24, 30, 0),
            )
            .expect("should resolve zoned datetime for overnight trip");
        assert_eq!(zoned_overnight.timezone(), chrono_tz::Tz::Europe__Paris);
        assert_eq!(
            zoned_overnight.date_naive(),
            NaiveDate::from_ymd_opt(2017, 1, 9).unwrap()
        );

        let timetable = RoutingTimetable::try_from(validated.clone()).unwrap();
        assert_eq!(
            timetable.feeds[0].primary_timezone,
            Some(chrono_tz::Tz::Europe__Paris)
        );
        assert_eq!(
            timetable.feed_timezone(FeedIdx::new(0)),
            Some(chrono_tz::Tz::Europe__Paris)
        );

        // Rider searches on Monday 2017-01-09 at 00:15:00 local time.
        // Sunday 2017-01-08 (yesterday) had `service1` active, and `overnight_trip` departs at 24:30:00
        let query_date = NaiveDate::from_ymd_opt(2017, 1, 9).unwrap();
        let query_time = GtfsTime::from_hms(0, 15, 0);
        let window = timetable.active_services_at_time(query_date, query_time);
        assert_eq!(
            window.yesterday_overnight_time,
            GtfsTime::from_hms(24, 15, 0)
        );

        let board = timetable
            .earliest_boardable_trip_with_overnight(PatternIdx::new(0), 0, &window)
            .expect("should board yesterday's overnight trip at 00:30 AM");
        assert!(board.is_overnight_from_yesterday);
        assert_eq!(board.departure_time, GtfsTime::from_hms(24, 30, 0));
        assert_eq!(
            board.normalized_query_day_departure(),
            GtfsTime::from_hms(0, 30, 0)
        );
        assert_eq!(
            board.departure_datetime(&timetable, query_date),
            Some(zoned_overnight)
        );
        assert_eq!(
            board.arrival_datetime_at(&timetable, query_date, 2),
            validated.resolve_trip_datetime(
                "overnight_trip",
                service_date,
                GtfsTime::from_hms(24, 50, 0)
            )
        );
    }

    #[test]
    fn test_active_services_for_feed_at_datetime_across_us_timezones() {
        let mut raw_ny = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        for ag in &mut raw_ny.agencies {
            ag.timezone = Tz::America__New_York;
        }
        let mut raw_la = RawFeed::from_path(fixture_path("fixtures/basic")).unwrap();
        for ag in &mut raw_la.agencies {
            ag.timezone = Tz::America__Los_Angeles;
        }

        let mut builder = RoutingTimetable::builder();
        let ny_idx = builder
            .add_feed("ny", ValidatedFeed::try_from(raw_ny).unwrap())
            .unwrap();
        let la_idx = builder
            .add_feed("la", ValidatedFeed::try_from(raw_la).unwrap())
            .unwrap();
        let timetable = builder.build();

        // 2017-01-08 06:30:00 UTC:
        // - In New York (UTC-5): 2017-01-08 (Sunday) at 01:30:00
        // - In Los Angeles (UTC-8): 2017-01-07 (Saturday) at 22:30:00
        let utc_instant = chrono::Utc.with_ymd_and_hms(2017, 1, 8, 6, 30, 0).unwrap();

        let ny_win = timetable
            .active_services_for_feed_at_datetime(ny_idx, &utc_instant)
            .unwrap();
        assert_eq!(ny_win.date, NaiveDate::from_ymd_opt(2017, 1, 8).unwrap());
        assert_eq!(ny_win.time_of_day, GtfsTime::from_hms(1, 30, 0));

        let la_win = timetable
            .active_services_for_feed_at_datetime(la_idx, &utc_instant)
            .unwrap();
        assert_eq!(la_win.date, NaiveDate::from_ymd_opt(2017, 1, 7).unwrap());
        assert_eq!(la_win.time_of_day, GtfsTime::from_hms(22, 30, 0));
    }
}
