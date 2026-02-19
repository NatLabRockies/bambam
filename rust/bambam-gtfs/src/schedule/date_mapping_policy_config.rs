use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::schedule::schedule_error::ScheduleError;

/// used to tag the type of mapping policy when constructing from CLI.
#[derive(Serialize, Deserialize, Clone, Debug, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DateMappingPolicyType {
    ExactDate,
    ExactRange,
    NearestDate,
    NearestRange,
    ExactDateTimeRange,
    NearestDateTimeRange,
    BestCase,
}

/// configures a [`DateMappingPolicy`]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum DateMappingPolicyConfig {
    ExactDate(String),
    ExactDateRange {
        /// start date in range
        start_date: String,
        end_date: String,
    },
    NearestDate {
        date: String,
        /// limit to the number of days to search from the target date +-
        /// to a viable date in the GTFS archive.
        date_tolerance: u64,
        /// if true, choose the closest date that matches the same day of the
        /// week as our target date.
        match_weekday: bool,
    },
    NearestDateRange {
        start_date: String,
        end_date: String,
        /// limit to the number of days to search from the target date +-
        /// to a viable date in the GTFS archive.
        date_tolerance: u64,
        /// if true, choose the closest date that matches the same day of the
        /// week as our target date.
        match_weekday: bool,
    },
    ExactDateTimeRange {
        /// start date in range
        start_date: String,
        end_date: String,
        start_time: String,
        end_time: String,
    },
    NearestDateTimeRange {
        start_date: String,
        end_date: String,
        start_time: String,
        end_time: String,
        /// limit to the number of days to search from the target date +-
        /// to a viable date in the GTFS archive.
        date_tolerance: u64,
        /// if true, choose the closest date that matches the same day of the
        /// week as our target date.
        match_weekday: bool,
    },
    BestCase {
        start_date: String,
        end_date: String,
        start_time: String,
        end_time: String,
        /// limit to the number of days to search from the target date +-
        /// to a viable date in the GTFS archive. default: +- 10 years.
        date_tolerance: Option<u64>,
    },
}

impl DateMappingPolicyConfig {
    /// build a new [`DateMappingPolicy`] configuration from CLI arguments.
    pub fn new(
        start_date: &str,
        end_date: &str,
        start_time: Option<&String>,
        end_time: Option<&String>,
        date_mapping_policy: &DateMappingPolicyType,
        date_mapping_date_tolerance: Option<u64>,
        date_mapping_match_weekday: Option<bool>,
    ) -> Result<DateMappingPolicyConfig, ScheduleError> {
        use DateMappingPolicyConfig as Config;
        use DateMappingPolicyType as Type;
        match date_mapping_policy {
            Type::ExactDate => Ok(Config::ExactDate(start_date.to_string())),
            Type::ExactRange => Ok(Config::ExactDateRange {
                start_date: start_date.to_string(),
                end_date: end_date.to_string(),
            }),
            Type::NearestDate => {
                let match_weekday = date_mapping_match_weekday.ok_or_else(|| ScheduleError::GtfsApp(String::from("for nearest-date mapping, must specify 'match_weekday' as 'true' or 'false'")))?;
                let date_tolerance = date_mapping_date_tolerance.ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "for nearest-date mapping, must specify a date_tolerance in [0, inf)",
                    ))
                })?;
                Ok(Self::NearestDate {
                    date: start_date.to_string(),
                    date_tolerance,
                    match_weekday,
                })
            }
            Type::NearestRange => {
                let match_weekday = date_mapping_match_weekday.ok_or_else(|| ScheduleError::GtfsApp(String::from("for nearest-date mapping, must specify 'match_weekday' as 'true' or 'false'")))?;
                let date_tolerance = date_mapping_date_tolerance.ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "for nearest-date mapping, must specify a date_tolerance in [0, inf)",
                    ))
                })?;
                Ok(Self::NearestDateRange {
                    start_date: start_date.to_string(),
                    end_date: end_date.to_string(),
                    date_tolerance,
                    match_weekday,
                })
            }
            Type::ExactDateTimeRange => {
                let start_time = start_time.cloned().ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "must provide start_time for exact date time range policy",
                    ))
                })?;
                let end_time = end_time.cloned().ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "must provide end_time for exact date time range policy",
                    ))
                })?;

                Ok(Config::ExactDateTimeRange {
                    start_date: start_date.to_string(),
                    end_date: end_date.to_string(),
                    start_time,
                    end_time,
                })
            }
            Type::NearestDateTimeRange => {
                let start_time = start_time.cloned().ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "must provide start_time for nearest date time range policy",
                    ))
                })?;
                let end_time = end_time.cloned().ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "must provide end_time for nearest date time range policy",
                    ))
                })?;
                let match_weekday = date_mapping_match_weekday.ok_or_else(|| ScheduleError::GtfsApp(String::from("for nearest-date mapping, must specify 'match_weekday' as 'true' or 'false'")))?;
                let date_tolerance = date_mapping_date_tolerance.ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "for nearest-date mapping, must specify a date_tolerance in [0, inf)",
                    ))
                })?;
                Ok(Self::NearestDateTimeRange {
                    start_date: start_date.to_string(),
                    end_date: end_date.to_string(),
                    start_time,
                    end_time,
                    date_tolerance,
                    match_weekday,
                })
            }
            Type::BestCase => {
                let start_time = start_time.cloned().ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "must provide start_time for best case policy",
                    ))
                })?;
                let end_time = end_time.cloned().ok_or_else(|| {
                    ScheduleError::GtfsApp(String::from(
                        "must provide end_time for best case policy",
                    ))
                })?;
                Ok(Self::BestCase {
                    start_date: start_date.to_string(),
                    end_date: end_date.to_string(),
                    start_time,
                    end_time,
                    date_tolerance: date_mapping_date_tolerance,
                })
            }
        }
    }
}
