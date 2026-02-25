use std::sync::Arc;

use chrono::{Datelike, NaiveDate, NaiveTime};
use gtfs_structures::{Exception, Gtfs, StopTime};
use itertools::Itertools;

use crate::schedule::{
    date::{
        date_codec::app::{APP_DATE_FORMAT, APP_TIME_FORMAT},
        date_ops, DateIterator,
    },
    DateMappingPolicyConfig,
};
use crate::schedule::{schedule_error::ScheduleError, SortedTrip};

#[derive(Clone, Debug)]
pub enum DateMappingPolicy {
    ExactDate(NaiveDate),
    ExactDateRange {
        /// start date in range
        start_date: NaiveDate,
        end_date: NaiveDate,
    },
    NearestDate {
        date: NaiveDate,
        /// limit to the number of days to search from the target date +-
        /// to a viable date in the GTFS archive.
        date_tolerance: u64,
        /// if true, choose the closest date that matches the same day of the
        /// week as our target date.
        match_weekday: bool,
    },
    NearestDateRange {
        start_date: NaiveDate,
        end_date: NaiveDate,
        /// limit to the number of days to search from the target date +-
        /// to a viable date in the GTFS archive.
        date_tolerance: u64,
        /// if true, choose the closest date that matches the same day of the
        /// week as our target date.
        match_weekday: bool,
    },
    ExactDatetimeRange {
        /// start datetime in range
        start_date: NaiveDate,
        end_date: NaiveDate,
        start_time: NaiveTime,
        end_time: NaiveTime,
    },
    NearestDatetimeRange {
        start_date: NaiveDate,
        end_date: NaiveDate,
        start_time: NaiveTime,
        end_time: NaiveTime,
        /// limit to the number of days to search from the target date +-
        /// to a viable date in the GTFS archive.
        date_tolerance: u64,
        /// if true, choose the closest date that matches the same day of the
        /// week as our target date.
        match_weekday: bool,
    },
    BestCase {
        start_date: NaiveDate,
        end_date: NaiveDate,
        start_time: NaiveTime,
        end_time: NaiveTime,
        date_tolerance: u64,
    },
}

impl TryFrom<&DateMappingPolicyConfig> for DateMappingPolicy {
    type Error = ScheduleError;

    fn try_from(value: &DateMappingPolicyConfig) -> Result<Self, Self::Error> {
        match value {
            DateMappingPolicyConfig::ExactDate(date_str) => {
                let date = NaiveDate::parse_from_str(date_str, APP_DATE_FORMAT).map_err(|e| {
                    ScheduleError::GtfsApp(format!(
                        "failure reading date for exact date mapping policy: {e}"
                    ))
                })?;
                Ok(Self::ExactDate(date))
            }
            DateMappingPolicyConfig::ExactDateRange {
                start_date,
                end_date,
            } => {
                let start_date =
                    NaiveDate::parse_from_str(start_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_date for exact range mapping policy: {e}"
                        ))
                    })?;
                let end_date =
                    NaiveDate::parse_from_str(end_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_date for exact range mapping policy: {e}"
                        ))
                    })?;
                Ok(Self::ExactDateRange {
                    start_date,
                    end_date,
                })
            }
            DateMappingPolicyConfig::NearestDate {
                date,
                date_tolerance,
                match_weekday,
            } => {
                let date = NaiveDate::parse_from_str(date, APP_DATE_FORMAT).map_err(|e| {
                    ScheduleError::GtfsApp(format!(
                        "failure reading date for nearest date mapping policy: {e}"
                    ))
                })?;
                Ok(Self::NearestDate {
                    date,
                    date_tolerance: *date_tolerance,
                    match_weekday: *match_weekday,
                })
            }
            DateMappingPolicyConfig::NearestDateRange {
                start_date,
                end_date,
                date_tolerance,
                match_weekday,
            } => {
                let start_date =
                    NaiveDate::parse_from_str(start_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_date for nearest range mapping policy: {e}"
                        ))
                    })?;
                let end_date =
                    NaiveDate::parse_from_str(end_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_date for nearest range mapping policy: {e}"
                        ))
                    })?;
                Ok(Self::NearestDateRange {
                    start_date,
                    end_date,
                    date_tolerance: *date_tolerance,
                    match_weekday: *match_weekday,
                })
            }
            DateMappingPolicyConfig::ExactDateTimeRange {
                start_date,
                end_date,
                start_time,
                end_time,
            } => {
                let start_date =
                    NaiveDate::parse_from_str(start_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_date for exact date time range mapping policy: {e}"
                        ))
                    })?;
                let end_date =
                    NaiveDate::parse_from_str(end_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_date for exact date time range mapping policy: {e}"
                        ))
                    })?;
                let start_time =
                    NaiveTime::parse_from_str(start_time, APP_TIME_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_time for exact date time range mapping policy: {e}"
                        ))
                    })?;
                let end_time =
                    NaiveTime::parse_from_str(end_time, APP_TIME_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_time for exact date time range mapping policy: {e}"
                        ))
                    })?;
                Ok(Self::ExactDatetimeRange {
                    start_date,
                    end_date,
                    start_time,
                    end_time,
                })
            }
            DateMappingPolicyConfig::NearestDateTimeRange {
                start_date,
                end_date,
                start_time,
                end_time,
                date_tolerance,
                match_weekday,
            } => {
                let start_date = NaiveDate::parse_from_str(start_date, APP_DATE_FORMAT)
                    .map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_date for nearest date time range mapping policy: {e}"
                        ))
                    })?;
                let end_date = NaiveDate::parse_from_str(end_date, APP_DATE_FORMAT)
                    .map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_date for nearest date time range mapping policy: {e}"
                        ))
                    })?;
                let start_time =
                    NaiveTime::parse_from_str(start_time, APP_TIME_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_time for nearest date time range mapping policy: {e}"
                        ))
                    })?;
                let end_time =
                    NaiveTime::parse_from_str(end_time, APP_TIME_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_time for nearest date time range mapping policy: {e}"
                        ))
                    })?;
                Ok(Self::NearestDatetimeRange {
                    start_date,
                    end_date,
                    start_time,
                    end_time,
                    date_tolerance: *date_tolerance,
                    match_weekday: *match_weekday,
                })
            }
            DateMappingPolicyConfig::BestCase {
                start_date,
                end_date,
                start_time,
                end_time,
                date_tolerance,
            } => {
                let start_date =
                    NaiveDate::parse_from_str(start_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_date for best case mapping policy: {e}"
                        ))
                    })?;
                let end_date =
                    NaiveDate::parse_from_str(end_date, APP_DATE_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_date for best case mapping policy: {e}"
                        ))
                    })?;
                let start_time =
                    NaiveTime::parse_from_str(start_time, APP_TIME_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading start_time for best case mapping policy: {e}"
                        ))
                    })?;
                let end_time =
                    NaiveTime::parse_from_str(end_time, APP_TIME_FORMAT).map_err(|e| {
                        ScheduleError::GtfsApp(format!(
                            "failure reading end_time for best case mapping policy: {e}"
                        ))
                    })?;
                let date_tolerance = date_tolerance.unwrap_or(10 * 365);
                Ok(Self::BestCase {
                    start_date,
                    end_date,
                    start_time,
                    end_time,
                    date_tolerance,
                })
            }
        }
    }
}

impl DateMappingPolicy {
    /// create an iterator over the dates we want to generate transit
    /// schedules for.
    pub fn iter(&self) -> DateIterator {
        match self {
            DateMappingPolicy::ExactDate(day) => DateIterator::new(*day, None),
            DateMappingPolicy::ExactDateRange {
                start_date,
                end_date,
            } => DateIterator::new(*start_date, Some(*end_date)),
            DateMappingPolicy::NearestDate { date, .. } => DateIterator::new(*date, None),
            DateMappingPolicy::NearestDateRange {
                start_date,
                end_date,
                ..
            } => DateIterator::new(*start_date, Some(*end_date)),
            DateMappingPolicy::ExactDatetimeRange {
                start_date,
                end_date,
                ..
            } => DateIterator::new(*start_date, Some(*end_date)),
            DateMappingPolicy::NearestDatetimeRange {
                start_date,
                end_date,
                ..
            } => DateIterator::new(*start_date, Some(*end_date)),
            DateMappingPolicy::BestCase {
                start_date,
                end_date,
                ..
            } => DateIterator::new(*start_date, Some(*end_date)),
        }
    }

    /// given some target date generated by the [`DateIterator`], we _pick_ a valid
    /// date according to the [`DateMappingPolicy`] variant's implementation + arguments.
    pub fn pick_date(
        &self,
        target: &NaiveDate,
        trip: &SortedTrip,
        gtfs: Arc<Gtfs>,
    ) -> Result<NaiveDate, ScheduleError> {
        match self {
            DateMappingPolicy::ExactDate(_) => pick_exact_date(target, trip, &gtfs),
            DateMappingPolicy::ExactDateRange { .. } => pick_exact_date(target, trip, &gtfs),
            DateMappingPolicy::NearestDate {
                date_tolerance,
                match_weekday,
                ..
            } => pick_nearest_date(target, trip, &gtfs, *date_tolerance, *match_weekday),
            DateMappingPolicy::NearestDateRange {
                date_tolerance,
                match_weekday,
                ..
            } => pick_nearest_date(target, trip, &gtfs, *date_tolerance, *match_weekday),
            DateMappingPolicy::ExactDatetimeRange { .. } => pick_exact_date(target, trip, &gtfs),
            DateMappingPolicy::NearestDatetimeRange {
                date_tolerance,
                match_weekday,
                ..
            } => pick_nearest_date(target, trip, &gtfs, *date_tolerance, *match_weekday),
            DateMappingPolicy::BestCase { date_tolerance, .. } => {
                let e1 = match pick_exact_date(target, trip, &gtfs) {
                    Ok(date) => return Ok(date),
                    Err(e) => e,
                };
                let e2 = match pick_nearest_date(target, trip, &gtfs, *date_tolerance, true) {
                    Ok(date) => return Ok(date),
                    Err(e) => e,
                };
                let e3 = match pick_nearest_date(target, trip, &gtfs, *date_tolerance, false) {
                    Ok(date) => return Ok(date),
                    Err(e) => e,
                };

                // all three strategies failed, return a detailed error message
                let msg = [
                    String::from("Failed to pick date with best_case strategy."),
                    format!("While attempting to pick exact date: {e1}."),
                    format!("While attempting to pick nearest date within {date_tolerance} days matching weekday: {e2}."),
                    format!("While attempting to pick nearest date within {date_tolerance} days without matching weekday: {e3}.")
                ].join("  ");
                Err(ScheduleError::InvalidData(msg))
            }
        }
    }

    /// confirms if a time for a trip stop is within the user-configured time of day range.
    /// always true when time ranges are not provided.
    ///
    /// assumes if we are matching times that we are picking a range within a 24-hour period without
    /// a change of dates.
    pub fn within_time_range(&self, src: &StopTime, dst: &StopTime) -> bool {
        match self {
            DateMappingPolicy::ExactDate(_) => true,
            DateMappingPolicy::ExactDateRange { .. } => true,
            DateMappingPolicy::NearestDate { .. } => true,
            DateMappingPolicy::NearestDateRange { .. } => true,
            DateMappingPolicy::ExactDatetimeRange {
                start_time,
                end_time,
                ..
            } => test_dst_arrival(src, dst, start_time, end_time),
            DateMappingPolicy::NearestDatetimeRange {
                start_time,
                end_time,
                ..
            } => test_dst_arrival(src, dst, start_time, end_time),
            DateMappingPolicy::BestCase {
                start_time,
                end_time,
                ..
            } => test_dst_arrival(src, dst, start_time, end_time),
        }
    }
}

/// confirm the target to exist as a valid date for this trip in the GTFS dataset.
/// returns the target date if successful.
fn pick_exact_date(
    target: &NaiveDate,
    trip: &SortedTrip,
    gtfs: &Gtfs,
) -> Result<NaiveDate, ScheduleError> {
    let c_opt = gtfs.get_calendar(&trip.service_id).ok();
    let cd_opt = gtfs.get_calendar_date(&trip.service_id).ok();
    match (c_opt, cd_opt) {
        (None, None) => {
            let msg = format!("cannot pick date with trip_id '{}' as it does not match calendar or calendar dates", trip.trip_id);
            Err(ScheduleError::MalformedGtfs(msg))
        }
        (Some(c), None) => date_ops::find_in_calendar(target, c),
        (None, Some(cd)) => date_ops::confirm_add_exception(target, cd),
        (Some(c), Some(cd)) => match date_ops::find_in_calendar(target, c) {
            Ok(_) => {
                if date_ops::confirm_no_delete_exception(target, cd) {
                    Ok(*target)
                } else {
                    Err(ScheduleError::InvalidData(format!(
                    "date {} is valid for calendar.txt but has exception of deleted in calendar_dates.txt",
                    target.format(APP_DATE_FORMAT)
                )))
                }
            }
            Err(ce) => date_ops::confirm_add_exception(target, cd)
                .map_err(|e| ScheduleError::InvalidData(format!("{ce}, {e}"))),
        },
    }
}

/// for date policies that search for the nearest valid dates to the target date by a threshold
/// and optionally enforce matching weekday.
fn pick_nearest_date(
    target: &NaiveDate,
    trip: &SortedTrip,
    gtfs: &Gtfs,
    date_tolerance: u64,
    match_weekday: bool,
) -> Result<NaiveDate, ScheduleError> {
    let c_opt = gtfs.get_calendar(&trip.service_id).ok();
    let cd_opt = gtfs.get_calendar_date(&trip.service_id).ok();
    match (c_opt, cd_opt) {
        (None, None) => {
            let msg = format!("cannot pick date with trip_id '{}' as it does not match calendar or calendar dates", trip.trip_id);
            Err(ScheduleError::MalformedGtfs(msg))
        }
        (None, Some(cd)) => {
            date_ops::find_nearest_add_exception(target, cd, date_tolerance, match_weekday)
        }
        (Some(c), None) => {
            let matches = date_ops::date_range_intersection(
                target,
                &c.start_date,
                &c.end_date,
                date_tolerance,
                match_weekday,
            )?;
            matches.first().cloned().ok_or_else(|| {
                let msg = date_ops::error_msg_suffix(target, &c.start_date, &c.end_date);
                ScheduleError::InvalidData(format!(
                    "could not find nearest (by {date_tolerance} days) date {msg}"
                ))
            })
        }
        (Some(c), Some(cd)) => {
            // find all matches across calendar.txt and calendar_dates.txt
            let mut matches = date_ops::date_range_intersection(
                target,
                &c.start_date,
                &c.end_date,
                date_tolerance,
                match_weekday,
            )?;
            // apply exceptions in calendar_dates.txt to the matches
            for calendar_date in cd.iter() {
                let matches_date = calendar_date.date == *target;
                let is_add = calendar_date.exception_type == Exception::Added;
                let matches_weekday_expectation =
                    !match_weekday || target.weekday() == calendar_date.date.weekday();
                if matches_date && is_add && matches_weekday_expectation {
                    matches.push(calendar_date.date);
                }
            }
            let matches_minus_delete = matches
                .into_iter()
                .filter(|date_match| date_ops::confirm_no_delete_exception(date_match, cd))
                .collect_vec();

            // find the valid date that is closest to the target date
            let min_distance_match = matches_minus_delete
                .iter()
                .map(|date| {
                    let days = target.signed_duration_since(*date).abs().num_days();
                    (days, date)
                })
                .min()
                .map(|(_, d)| d)
                .cloned();

            min_distance_match.ok_or_else(|| {
                ScheduleError::InvalidData(format!(
                    "no match found across calendar + calendar_dates {}",
                    date_ops::error_msg_suffix(target, &c.start_date, &c.end_date)
                ))
            })
        }
    }
}

/// helper function to check if a source and destination stop time pair is within some time range.
/// false if either time value exists but is outside of [0, 86399].
/// does not fail if times are not specified, but instead optimisically returns true.
fn test_dst_arrival(
    src: &StopTime,
    dst: &StopTime,
    start_time: &NaiveTime,
    end_time: &NaiveTime,
) -> bool {
    let is_within_time_range = |t: &NaiveTime| start_time <= t && t <= end_time;
    match (src.departure_time, dst.arrival_time) {
        (None, None) => true,
        (None, Some(arr)) => match NaiveTime::from_num_seconds_from_midnight_opt(arr, 0) {
            Some(t) => is_within_time_range(&t),
            None => false,
        },
        (Some(dep), None) => match NaiveTime::from_num_seconds_from_midnight_opt(dep, 0) {
            Some(t) => is_within_time_range(&t),
            None => false,
        },
        (Some(dep), Some(arr)) => {
            let dep_t = NaiveTime::from_num_seconds_from_midnight_opt(dep, 0);
            let arr_t = NaiveTime::from_num_seconds_from_midnight_opt(arr, 0);
            match (dep_t, arr_t) {
                (Some(t0), Some(t1)) => is_within_time_range(&t0) && is_within_time_range(&t1),
                _ => false,
            }
        }
    }
}
