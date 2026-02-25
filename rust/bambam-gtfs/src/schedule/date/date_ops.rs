use std::collections::BinaryHeap;

use chrono::{Datelike, Days, NaiveDate};
use gtfs_structures::{Calendar, CalendarDate, Exception};

use crate::schedule::{
    date::date_codec::app::APP_DATE_FORMAT, date::DateCandidate, schedule_error::ScheduleError,
};

/// tests intersection (inclusive) of some target date with a date range.
///
/// # Arguments
///
/// * `target` - the "real" date from the date range
/// * `start` - start (inclusive) of the range to test against from the calendar.txt file
/// * `end` - end (inclusive) of the range to test against from the calendar.txt file
/// * `date_tolerance` - number of days outside of range we will tolerate remapping dates
/// * `match_weekday` - if true, ensure any found dates also match the target by day of week
///
/// # Returns
///
/// all valid dates that could be used from this date range, filtering by the date
/// tolerance and weekday match criteria.
pub fn date_range_intersection(
    target: &NaiveDate,
    start: &NaiveDate,
    end: &NaiveDate,
    date_tolerance: u64,
    match_weekday: bool,
) -> Result<Vec<NaiveDate>, ScheduleError> {
    let mut candidates: Vec<(u64, NaiveDate)> = Vec::new();

    // Calculate the tolerance range around the target date
    let target_start = step_date(*target, -(date_tolerance as i64))?;
    let target_end = step_date(*target, date_tolerance as i64)?;

    // Find the overlap between [target_start, target_end] and [start, end]
    let overlap_start = std::cmp::max(target_start, *start);
    let overlap_end = std::cmp::min(target_end, *end);

    // If there's no overlap, return empty vector
    if overlap_start > overlap_end {
        return Ok(Vec::new());
    }

    // Iterate through the overlapping date range
    let mut current = overlap_start;
    while current <= overlap_end {
        let distance = current.signed_duration_since(*target).abs().num_days() as u64;

        // Check if this date is within tolerance
        if distance <= date_tolerance {
            // Check weekday matching if required
            if !match_weekday || current.weekday() == target.weekday() {
                candidates.push((distance, current));
            }
        }

        // Move to next day
        current = step_date(current, 1)?;
    }

    // Sort by distance from target (closest first)
    candidates.sort_by_key(|(distance, _)| *distance);

    // Extract just the dates from the (distance, date) tuples
    Ok(candidates.into_iter().map(|(_, date)| date).collect())
}

/// helper function to find some expected date in the calendar.txt of a GTFS archive
pub fn find_in_calendar(
    target: &NaiveDate,
    calendar: &Calendar,
) -> Result<NaiveDate, ScheduleError> {
    let start = &calendar.start_date;
    let end = &calendar.end_date;
    let within_service_date_range = start <= target && target <= end;
    if within_service_date_range {
        Ok(*target)
    } else {
        let msg = error_msg_suffix(target, start, end);
        Err(ScheduleError::InvalidData(format!(
            "no calendar.txt dates match {msg}"
        )))
    }
}

/// helper function to find some expected target date in the calendar_dates.txt of a
/// GTFS archive where the entry should have an exception_type of "Added".
pub fn confirm_add_exception(
    target: &NaiveDate,
    calendar_dates: &[CalendarDate],
) -> Result<NaiveDate, ScheduleError> {
    match calendar_dates
        .iter()
        .find(|cd| &cd.date == target && cd.exception_type == Exception::Added)
    {
        Some(_) => Ok(*target),
        None => {
            let msg = format!(
                "no calendar_dates match target date '{}' with exception_type as 'added'",
                target.format(APP_DATE_FORMAT),
            );
            Err(ScheduleError::InvalidData(msg))
        }
    }
}

/// helper function to find some expected target date in the calendar_dates.txt of a
/// GTFS archive where the entry should
///   1) not exist, or
///   2) NOT have an exception_type of "Deleted".
pub fn confirm_no_delete_exception(target: &NaiveDate, calendar_dates: &[CalendarDate]) -> bool {
    !calendar_dates
        .iter()
        .any(|cd| &cd.date == target && cd.exception_type == Exception::Deleted)
}

/// finds the nearest date to the target date that has an exception_type of "Added"
/// which is within some date_tolerance.
pub fn find_nearest_add_exception(
    target: &NaiveDate,
    calendar_dates: &[CalendarDate],
    date_tolerance: u64,
    match_weekday: bool,
) -> Result<NaiveDate, ScheduleError> {
    let mut heap = BinaryHeap::new();
    for date in calendar_dates.iter() {
        let matches_exception = date.exception_type == Exception::Added;
        let matches_weekday = if match_weekday {
            date.date.weekday() == target.weekday()
        } else {
            true
        };

        if matches_exception && matches_weekday {
            let time_delta = date.date.signed_duration_since(*target).abs();
            let days = time_delta.num_days() as u64;
            if days <= date_tolerance {
                heap.push(DateCandidate(days, date.clone()));
            }
        }
    }
    match heap.pop() {
        Some(min_distance_date) => Ok(min_distance_date.1.date),
        None => {
            let mwd_str = if match_weekday {
                " with matching weekday"
            } else {
                ""
            };
            let msg = format!(
                "no Added entry in calendar_dates.txt within {date_tolerance} days of {}{}",
                target.format(APP_DATE_FORMAT),
                mwd_str
            );
            Err(ScheduleError::InvalidData(msg))
        }
    }
}

/// adds (or when step is negative, subtracts) days from a date.
pub fn step_date(date: NaiveDate, step: i64) -> Result<NaiveDate, ScheduleError> {
    if step == 0 {
        return Ok(date);
    }
    let stepped = if step < 0 {
        let step_days = Days::new(step.unsigned_abs());
        date.checked_sub_days(step_days)
    } else {
        let step_days = Days::new(step.unsigned_abs());
        date.checked_add_days(step_days)
    };
    stepped.ok_or_else(|| {
        let op = if step < 0 { "subtracting" } else { "adding" };
        let msg = format!(
            "failure {} {} days to date {} due to bounds error",
            op,
            step,
            date.format(APP_DATE_FORMAT)
        );
        ScheduleError::InvalidData(msg)
    })
}

/// helper function for returning errors that reference some target date and date range
pub fn error_msg_suffix(target: &NaiveDate, start: &NaiveDate, end: &NaiveDate) -> String {
    format!(
        "for target date '{}' and date range [{},{}]",
        target.format(APP_DATE_FORMAT),
        start.format(APP_DATE_FORMAT),
        end.format(APP_DATE_FORMAT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_step_date_zero_step() {
        let date = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let result = step_date(date, 0).unwrap();
        assert_eq!(result, date);
    }

    #[test]
    fn test_step_date_positive_step() {
        let date = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let expected = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap();
        let result = step_date(date, 5).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_step_date_negative_step() {
        let date = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let expected = NaiveDate::from_ymd_opt(2023, 6, 10).unwrap();
        let result = step_date(date, -5).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_step_date_overflow_positive() {
        // Test with a date close to the maximum representable date
        let date = NaiveDate::MAX;
        let result = step_date(date, 1);
        assert!(result.is_err());

        if let Err(ScheduleError::InvalidData(msg)) = result {
            assert!(msg.contains("failure adding"));
            assert!(msg.contains("bounds error"));
        } else {
            panic!("Expected InvalidDataError for overflow");
        }
    }

    #[test]
    fn test_step_date_overflow_negative() {
        // Test with a date close to the minimum representable date
        let date = NaiveDate::MIN;
        let result = step_date(date, -1);
        assert!(result.is_err());

        if let Err(ScheduleError::InvalidData(msg)) = result {
            assert!(msg.contains("failure subtracting"));
            assert!(msg.contains("bounds error"));
        } else {
            panic!("Expected InvalidDataError for underflow");
        }
    }

    // Tests for confirm_add_exception
    #[test]
    fn test_confirm_add_exception_success() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(),
                Exception::Added,
            ),
            create_calendar_date(target, Exception::Added),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Deleted,
            ),
        ];

        let result = confirm_add_exception(&target, &calendar_dates);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), target);
    }

    #[test]
    fn test_confirm_add_exception_not_found() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(),
                Exception::Added,
            ),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Deleted,
            ),
        ];

        let result = confirm_add_exception(&target, &calendar_dates);
        assert!(result.is_err());
        if let Err(ScheduleError::InvalidData(msg)) = result {
            assert!(msg.contains("no calendar_dates match target date"));
            assert!(msg.contains("06-15-2023")); // MM-DD-YYYY format
            assert!(msg.contains("exception_type as 'added'"));
        } else {
            panic!("Expected InvalidDataError");
        }
    }

    #[test]
    fn test_confirm_add_exception_deleted_entry_exists() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(target, Exception::Deleted), // Has the date but wrong exception type
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Added,
            ),
        ];

        let result = confirm_add_exception(&target, &calendar_dates);
        assert!(result.is_err());
    }

    #[test]
    fn test_confirm_add_exception_empty_calendar_dates() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![];

        let result = confirm_add_exception(&target, &calendar_dates);
        assert!(result.is_err());
    }

    // Tests for confirm_no_delete_exception
    #[test]
    fn test_confirm_no_delete_exception_true() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(),
                Exception::Added,
            ),
            create_calendar_date(target, Exception::Added),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Added,
            ),
        ];

        let result = confirm_no_delete_exception(&target, &calendar_dates);
        assert!(result);
    }

    #[test]
    fn test_confirm_no_delete_exception_false() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(),
                Exception::Added,
            ),
            create_calendar_date(target, Exception::Deleted),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Added,
            ),
        ];

        let result = confirm_no_delete_exception(&target, &calendar_dates);
        assert!(!result);
    }

    #[test]
    fn test_confirm_no_delete_exception_date_not_present() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(),
                Exception::Added,
            ),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Deleted,
            ),
        ];

        let result = confirm_no_delete_exception(&target, &calendar_dates);
        assert!(result); // True because target date is not in the list
    }

    #[test]
    fn test_confirm_no_delete_exception_empty_calendar_dates() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![];

        let result = confirm_no_delete_exception(&target, &calendar_dates);
        assert!(result); // True because no entries means no delete exceptions
    }

    // Tests for find_nearest_add_exception
    #[test]
    fn test_find_nearest_add_exception_exact_match() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(),
                Exception::Added,
            ),
            create_calendar_date(target, Exception::Added),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Added,
            ),
        ];

        let result = find_nearest_add_exception(&target, &calendar_dates, 0, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), target);
    }

    #[test]
    fn test_find_nearest_add_exception_nearest_within_tolerance() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(); // Friday
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 12).unwrap(),
                Exception::Added,
            ), // 3 days before
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 18).unwrap(),
                Exception::Added,
            ), // 3 days after
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(),
                Exception::Deleted,
            ), // Should be ignored
        ];

        let result = find_nearest_add_exception(&target, &calendar_dates, 5, false);
        assert!(result.is_ok());
        // Should return the closer one (6/12, which is 3 days away)
        assert_eq!(
            result.unwrap(),
            NaiveDate::from_ymd_opt(2023, 6, 12).unwrap()
        );
    }

    #[test]
    fn test_find_nearest_add_exception_with_weekday_matching() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(); // Thursday
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 12).unwrap(),
                Exception::Added,
            ), // Monday, 3 days before
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 8).unwrap(),
                Exception::Added,
            ), // Thursday, 7 days before
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 22).unwrap(),
                Exception::Added,
            ), // Thursday, 7 days after
        ];

        let result = find_nearest_add_exception(&target, &calendar_dates, 10, true);
        assert!(result.is_ok());
        // Should return the closer Thursday (6/8, which is 7 days away but matches weekday)
        assert_eq!(
            result.unwrap(),
            NaiveDate::from_ymd_opt(2023, 6, 8).unwrap()
        );
    }

    #[test]
    fn test_find_nearest_add_exception_outside_tolerance() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 1).unwrap(),
                Exception::Added,
            ), // 14 days before
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 30).unwrap(),
                Exception::Added,
            ), // 15 days after
        ];

        let result = find_nearest_add_exception(&target, &calendar_dates, 5, false);
        assert!(result.is_err());
        if let Err(ScheduleError::InvalidData(msg)) = result {
            assert!(msg.contains("no Added entry in calendar_dates.txt"));
            assert!(msg.contains("within 5 days"));
            assert!(msg.contains("06-15-2023")); // MM-DD-YYYY format
        } else {
            panic!("Expected InvalidDataError");
        }
    }

    #[test]
    fn test_find_nearest_add_exception_no_weekday_match() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(); // Thursday
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 12).unwrap(),
                Exception::Added,
            ), // Monday
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 16).unwrap(),
                Exception::Added,
            ), // Friday
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 18).unwrap(),
                Exception::Added,
            ), // Sunday
        ];

        let result = find_nearest_add_exception(&target, &calendar_dates, 5, true);
        assert!(result.is_err());
        if let Err(ScheduleError::InvalidData(msg)) = result {
            assert!(msg.contains("no Added entry in calendar_dates.txt"));
            assert!(msg.contains("with matching weekday"));
        } else {
            panic!("Expected InvalidDataError");
        }
    }

    #[test]
    fn test_find_nearest_add_exception_only_deleted_entries() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 14).unwrap(),
                Exception::Deleted,
            ),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(),
                Exception::Deleted,
            ),
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 16).unwrap(),
                Exception::Deleted,
            ),
        ];

        let result = find_nearest_add_exception(&target, &calendar_dates, 5, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_nearest_add_exception_empty_calendar_dates() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![];

        let result = find_nearest_add_exception(&target, &calendar_dates, 5, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_nearest_add_exception_equal_distance_picks_earlier() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 12).unwrap(),
                Exception::Added,
            ), // 3 days before
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 18).unwrap(),
                Exception::Added,
            ), // 3 days after
        ];

        let result = find_nearest_add_exception(&target, &calendar_dates, 5, false);
        assert!(result.is_ok());
        // With the reversed ordering, when distances are equal, it should pick the earlier date
        // because of the tie-breaker: other.1.date.cmp(&self.1.date)
        assert_eq!(
            result.unwrap(),
            NaiveDate::from_ymd_opt(2023, 6, 12).unwrap()
        );
    }

    #[test]
    fn test_find_nearest_add_exception_tolerance_boundary_behavior() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar_dates = vec![
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(),
                Exception::Added,
            ), // Exactly 5 days before
            create_calendar_date(
                NaiveDate::from_ymd_opt(2023, 6, 9).unwrap(),
                Exception::Added,
            ), // 6 days before (outside tolerance)
        ];

        // With tolerance 4, the date that is exactly 4 days away should be excluded
        let result = find_nearest_add_exception(&target, &calendar_dates, 4, false);
        assert!(result.is_err()); // 5 days should be outside tolerance

        // But with tolerance 5, it should be included
        let result = find_nearest_add_exception(&target, &calendar_dates, 5, false);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            NaiveDate::from_ymd_opt(2023, 6, 10).unwrap()
        );
    }

    // Tests for find_in_calendar
    #[test]
    fn test_find_in_calendar_target_within_range() {
        let start = NaiveDate::from_ymd_opt(2023, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar = create_calendar(start, end);

        let result = find_in_calendar(&target, &calendar);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), target);
    }

    #[test]
    fn test_find_in_calendar_target_at_start() {
        let start = NaiveDate::from_ymd_opt(2023, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();
        let target = start;
        let calendar = create_calendar(start, end);

        let result = find_in_calendar(&target, &calendar);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), target);
    }

    #[test]
    fn test_find_in_calendar_target_at_end() {
        let start = NaiveDate::from_ymd_opt(2023, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();
        let target = end;
        let calendar = create_calendar(start, end);

        let result = find_in_calendar(&target, &calendar);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), target);
    }

    #[test]
    fn test_find_in_calendar_target_before_range() {
        let start = NaiveDate::from_ymd_opt(2023, 6, 10).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();
        let target = NaiveDate::from_ymd_opt(2023, 6, 5).unwrap(); // Before start
        let calendar = create_calendar(start, end);

        let result = find_in_calendar(&target, &calendar);
        assert!(result.is_err());
        if let Err(ScheduleError::InvalidData(msg)) = result {
            assert!(msg.contains("no calendar.txt dates match"));
            assert!(msg.contains("06-05-2023"));
            assert!(msg.contains("[06-10-2023,06-30-2023]"));
        } else {
            panic!("Expected InvalidDataError");
        }
    }

    #[test]
    fn test_find_in_calendar_target_after_range() {
        let start = NaiveDate::from_ymd_opt(2023, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap();
        let target = NaiveDate::from_ymd_opt(2023, 6, 25).unwrap(); // After end
        let calendar = create_calendar(start, end);

        let result = find_in_calendar(&target, &calendar);
        assert!(result.is_err());
        if let Err(ScheduleError::InvalidData(msg)) = result {
            assert!(msg.contains("no calendar.txt dates match"));
            assert!(msg.contains("06-25-2023"));
            assert!(msg.contains("[06-01-2023,06-20-2023]"));
        } else {
            panic!("Expected InvalidDataError");
        }
    }

    #[test]
    fn test_find_in_calendar_single_day_range_match() {
        let date = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let calendar = create_calendar(date, date);

        let result = find_in_calendar(&date, &calendar);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), date);
    }

    #[test]
    fn test_find_in_calendar_single_day_range_no_match() {
        let calendar_date = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let target = NaiveDate::from_ymd_opt(2023, 6, 16).unwrap();
        let calendar = create_calendar(calendar_date, calendar_date);

        let result = find_in_calendar(&target, &calendar);
        assert!(result.is_err());
    }

    // Tests for date_range_intersection
    #[test]
    fn test_date_range_intersection_target_within_range() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 6, 10).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap();

        let result = date_range_intersection(&target, &start, &end, 3, false).unwrap();

        // Should include dates within 3 days of target that are also in the range, sorted by distance
        let expected = vec![
            NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(), // 0 days
            NaiveDate::from_ymd_opt(2023, 6, 14).unwrap(), // 1 day before
            NaiveDate::from_ymd_opt(2023, 6, 16).unwrap(), // 1 day after
            NaiveDate::from_ymd_opt(2023, 6, 13).unwrap(), // 2 days before
            NaiveDate::from_ymd_opt(2023, 6, 17).unwrap(), // 2 days after
            NaiveDate::from_ymd_opt(2023, 6, 12).unwrap(), // 3 days before
            NaiveDate::from_ymd_opt(2023, 6, 18).unwrap(), // 3 days after
        ];
        assert_eq!(result, expected);
    }
    #[test]
    fn test_date_range_intersection_tolerance_zero() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 6, 10).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap();

        let result = date_range_intersection(&target, &start, &end, 0, false).unwrap();

        // Should only include the target date itself
        let expected = vec![target];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_date_range_intersection_no_overlap() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(); // Range starts after tolerance window
        let end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();

        let result = date_range_intersection(&target, &start, &end, 3, false).unwrap();

        // No overlap between [2023-06-12, 2023-06-18] and [2023-06-20, 2023-06-30]
        assert!(result.is_empty());
    }

    #[test]
    fn test_date_range_intersection_partial_overlap() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 6, 17).unwrap(); // Range starts partway through tolerance window
        let end = NaiveDate::from_ymd_opt(2023, 6, 25).unwrap();

        let result = date_range_intersection(&target, &start, &end, 3, false).unwrap();

        // Overlap is [2023-06-17, 2023-06-18] (only the part of tolerance window that's in range)
        let expected = vec![
            NaiveDate::from_ymd_opt(2023, 6, 17).unwrap(), // 2 days after target
            NaiveDate::from_ymd_opt(2023, 6, 18).unwrap(), // 3 days after target
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_date_range_intersection_with_weekday_matching() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(); // Thursday
        let start = NaiveDate::from_ymd_opt(2023, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();

        let result = date_range_intersection(&target, &start, &end, 10, true).unwrap();

        // Should only include Thursdays within 10 days of target, sorted by distance
        let expected = vec![
            NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(), // Thursday, target (0 days)
            NaiveDate::from_ymd_opt(2023, 6, 8).unwrap(),  // Thursday, 7 days before
            NaiveDate::from_ymd_opt(2023, 6, 22).unwrap(), // Thursday, 7 days after
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_date_range_intersection_weekday_no_matches() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(); // Thursday
        let start = NaiveDate::from_ymd_opt(2023, 6, 12).unwrap(); // Monday
        let end = NaiveDate::from_ymd_opt(2023, 6, 14).unwrap(); // Wednesday

        let result = date_range_intersection(&target, &start, &end, 5, true).unwrap();

        // No Thursdays in the range [Monday, Wednesday]
        assert!(result.is_empty());
    }

    #[test]
    fn test_date_range_intersection_range_entirely_before_tolerance() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 6, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(); // Ends before tolerance window starts

        let result = date_range_intersection(&target, &start, &end, 3, false).unwrap();

        // No overlap between [2023-06-01, 2023-06-10] and [2023-06-12, 2023-06-18]
        assert!(result.is_empty());
    }

    #[test]
    fn test_date_range_intersection_range_entirely_after_tolerance() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(); // Starts after tolerance window ends
        let end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();

        let result = date_range_intersection(&target, &start, &end, 3, false).unwrap();

        // No overlap between [2023-06-20, 2023-06-30] and [2023-06-12, 2023-06-18]
        assert!(result.is_empty());
    }

    #[test]
    fn test_date_range_intersection_single_day_range() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let single_day = NaiveDate::from_ymd_opt(2023, 6, 17).unwrap(); // 2 days after target

        let result = date_range_intersection(&target, &single_day, &single_day, 3, false).unwrap();

        // Single day range that's within tolerance
        let expected = vec![single_day];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_date_range_intersection_single_day_outside_tolerance() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let single_day = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(); // 5 days after target

        let result = date_range_intersection(&target, &single_day, &single_day, 3, false).unwrap();

        // Single day range that's outside tolerance
        assert!(result.is_empty());
    }

    #[test]
    fn test_date_range_intersection_sorted_by_distance() {
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 6, 10).unwrap();
        let end = NaiveDate::from_ymd_opt(2023, 6, 20).unwrap();

        let result = date_range_intersection(&target, &start, &end, 5, false).unwrap();

        // Results should be sorted by distance from target (closest first)
        let expected = vec![
            NaiveDate::from_ymd_opt(2023, 6, 15).unwrap(), // 0 days
            NaiveDate::from_ymd_opt(2023, 6, 14).unwrap(), // 1 day before
            NaiveDate::from_ymd_opt(2023, 6, 16).unwrap(), // 1 day after
            NaiveDate::from_ymd_opt(2023, 6, 13).unwrap(), // 2 days before
            NaiveDate::from_ymd_opt(2023, 6, 17).unwrap(), // 2 days after
            NaiveDate::from_ymd_opt(2023, 6, 12).unwrap(), // 3 days before
            NaiveDate::from_ymd_opt(2023, 6, 18).unwrap(), // 3 days after
            NaiveDate::from_ymd_opt(2023, 6, 11).unwrap(), // 4 days before
            NaiveDate::from_ymd_opt(2023, 6, 19).unwrap(), // 4 days after
            NaiveDate::from_ymd_opt(2023, 6, 10).unwrap(), // 5 days before
            NaiveDate::from_ymd_opt(2023, 6, 20).unwrap(), // 5 days after
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_date_range_intersection_year_boundary() {
        let target = NaiveDate::from_ymd_opt(2023, 12, 30).unwrap();
        let start = NaiveDate::from_ymd_opt(2023, 12, 28).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(); // Crosses year boundary

        let result = date_range_intersection(&target, &start, &end, 3, false).unwrap();

        // Should handle year boundary correctly
        let expected = vec![
            NaiveDate::from_ymd_opt(2023, 12, 30).unwrap(), // 0 days
            NaiveDate::from_ymd_opt(2023, 12, 29).unwrap(), // 1 day before
            NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(), // 1 day after
            NaiveDate::from_ymd_opt(2023, 12, 28).unwrap(), // 2 days before
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),   // 2 days after
            NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),   // 3 days after
        ];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_date_range_intersection_leap_year() {
        let target = NaiveDate::from_ymd_opt(2024, 2, 28).unwrap(); // 2024 is a leap year
        let start = NaiveDate::from_ymd_opt(2024, 2, 27).unwrap();
        let end = NaiveDate::from_ymd_opt(2024, 3, 2).unwrap();

        let result = date_range_intersection(&target, &start, &end, 3, false).unwrap();

        // Should handle leap year correctly
        let expected = vec![
            NaiveDate::from_ymd_opt(2024, 2, 28).unwrap(), // 0 days
            NaiveDate::from_ymd_opt(2024, 2, 27).unwrap(), // 1 day before
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(), // 1 day after (leap day)
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),  // 2 days after
            NaiveDate::from_ymd_opt(2024, 3, 2).unwrap(),  // 3 days after
        ];
        assert_eq!(result, expected);
    }

    // Helper function to create test CalendarDate TODO move
    fn create_calendar_date(date: NaiveDate, exception_type: Exception) -> CalendarDate {
        CalendarDate {
            service_id: "test_service".to_string(),
            date,
            exception_type,
        }
    }

    // Helper function to create test Calendar
    fn create_calendar(start_date: NaiveDate, end_date: NaiveDate) -> Calendar {
        Calendar {
            id: "test_service".to_string(),
            start_date,
            end_date,
            monday: true,
            tuesday: true,
            wednesday: true,
            thursday: true,
            friday: true,
            saturday: true,
            sunday: true,
        }
    }
}
