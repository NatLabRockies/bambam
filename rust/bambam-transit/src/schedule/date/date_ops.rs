use chrono::{Days, NaiveDate};

use crate::schedule::{date::date_codec::app::APP_DATE_FORMAT, schedule_error::ScheduleError};

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
}
