use chrono::NaiveDate;

use crate::schedule::date::date_ops;

pub struct DateIterator {
    current: Option<NaiveDate>,
    end_inclusive: NaiveDate,
}

impl DateIterator {
    pub fn new(start: NaiveDate, end: Option<NaiveDate>) -> DateIterator {
        DateIterator {
            current: Some(start),
            end_inclusive: end.unwrap_or(start),
        }
    }
}

impl Iterator for DateIterator {
    type Item = NaiveDate;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        if current > self.end_inclusive {
            return None; // prevent unbounded iteration with faulty arguments
        }
        let next_current = date_ops::step_date(current, 1).ok();
        self.current = next_current;
        Some(current)
    }
}
