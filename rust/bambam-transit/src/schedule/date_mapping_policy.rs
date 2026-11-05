use bambam_gtfs::{model::StopTime, TemporalStopTime, ValidatedFeed as Gtfs};
use chrono::{NaiveDate, NaiveTime};

use crate::schedule::schedule_error::ScheduleError;
use crate::schedule::{
    date::{
        date_codec::app::{APP_DATE_FORMAT, APP_TIME_FORMAT},
        DateIterator,
    },
    DateMappingPolicyConfig,
};

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

    /// Converts this schedule policy's date-matching rules into a [`bambam_gtfs::DateMappingPolicy`].
    pub fn to_gtfs_policy(&self) -> bambam_gtfs::DateMappingPolicy {
        match self {
            DateMappingPolicy::ExactDate(_)
            | DateMappingPolicy::ExactDateRange { .. }
            | DateMappingPolicy::ExactDatetimeRange { .. } => bambam_gtfs::DateMappingPolicy::Exact,
            DateMappingPolicy::NearestDate {
                date_tolerance,
                match_weekday,
                ..
            }
            | DateMappingPolicy::NearestDateRange {
                date_tolerance,
                match_weekday,
                ..
            }
            | DateMappingPolicy::NearestDatetimeRange {
                date_tolerance,
                match_weekday,
                ..
            } => bambam_gtfs::DateMappingPolicy::Nearest {
                tolerance_days: *date_tolerance,
                match_weekday: *match_weekday,
            },
            DateMappingPolicy::BestCase { date_tolerance, .. } => {
                bambam_gtfs::DateMappingPolicy::BestCase {
                    tolerance_days: *date_tolerance,
                }
            }
        }
    }

    /// Resolves `target` to a single representative active service date (`picked_date`) across
    /// all [`bambam_gtfs::ServiceCalendar`]s in `gtfs`.
    pub fn resolve_feed_date(
        &self,
        target: &NaiveDate,
        gtfs: &Gtfs,
    ) -> Result<NaiveDate, ScheduleError> {
        gtfs.resolve_service_date(*target, self.to_gtfs_policy())
            .ok_or_else(|| {
                ScheduleError::InvalidData(format!(
                    "no active service date found in feed for target date '{}' with policy {:?}",
                    target.format(APP_DATE_FORMAT),
                    self
                ))
            })
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
    match (
        src.departure_time().map(|t| t.seconds()),
        dst.arrival_time().map(|t| t.seconds()),
    ) {
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
