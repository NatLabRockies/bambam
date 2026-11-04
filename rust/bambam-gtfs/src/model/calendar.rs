//! Unified `ServiceCalendar` combining weekly patterns (`calendar.txt`) and date exceptions (`calendar_dates.txt`).

use crate::enums::Exception;
use crate::ids::ServiceId;
use crate::raw::{RawCalendar, RawCalendarDate};
use crate::traits::Identifiable;
use chrono::{Datelike, NaiveDate, Weekday};
use std::collections::HashSet;

/// Recurring weekly operating pattern from `calendar.txt`.
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
pub struct WeeklyPattern {
    /// Bitmask of active weekdays (`bit 0 = Monday` .. `bit 6 = Sunday`).
    pub weekday_mask: u8,
    /// First date of the service interval (inclusive).
    #[rkyv(with = crate::primitives::RkyvNaiveDate)]
    pub start_date: NaiveDate,
    /// Last date of the service interval (inclusive).
    #[rkyv(with = crate::primitives::RkyvNaiveDate)]
    pub end_date: NaiveDate,
}

impl WeeklyPattern {
    /// Returns true if the weekly pattern runs on `weekday`.
    #[inline]
    pub const fn runs_on_weekday(&self, weekday: Weekday) -> bool {
        let bit = match weekday {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        };
        (self.weekday_mask & (1 << bit)) != 0
    }
}

impl From<&RawCalendar> for WeeklyPattern {
    fn from(raw: &RawCalendar) -> Self {
        let mut mask = 0u8;
        if raw.monday {
            mask |= 1 << 0;
        }
        if raw.tuesday {
            mask |= 1 << 1;
        }
        if raw.wednesday {
            mask |= 1 << 2;
        }
        if raw.thursday {
            mask |= 1 << 3;
        }
        if raw.friday {
            mask |= 1 << 4;
        }
        if raw.saturday {
            mask |= 1 << 5;
        }
        if raw.sunday {
            mask |= 1 << 6;
        }
        Self {
            weekday_mask: mask,
            start_date: raw.start_date,
            end_date: raw.end_date,
        }
    }
}

/// Unified service schedule for a `ServiceId` combining `calendar.txt` and `calendar_dates.txt`.
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
pub struct ServiceCalendar {
    /// Unique service identifier.
    pub id: ServiceId,
    /// Optional base weekly pattern from `calendar.txt`.
    pub weekly: Option<WeeklyPattern>,
    /// Specific dates added via `calendar_dates.txt` (`exception_type = 1`).
    #[rkyv(with = crate::primitives::RkyvNaiveDateSet)]
    pub added_dates: HashSet<NaiveDate>,
    /// Specific dates removed via `calendar_dates.txt` (`exception_type = 2`).
    #[rkyv(with = crate::primitives::RkyvNaiveDateSet)]
    pub removed_dates: HashSet<NaiveDate>,
}

impl ServiceCalendar {
    /// Creates an empty `ServiceCalendar` for `id`.
    pub fn new(id: ServiceId) -> Self {
        Self {
            id,
            weekly: None,
            added_dates: HashSet::new(),
            removed_dates: HashSet::new(),
        }
    }

    /// Applies a `calendar_dates.txt` exception to this service calendar.
    pub fn add_exception(&mut self, cd: &RawCalendarDate) {
        match cd.exception_type {
            Exception::Added => {
                self.removed_dates.remove(&cd.date);
                self.added_dates.insert(cd.date);
            }
            Exception::Deleted => {
                self.added_dates.remove(&cd.date);
                self.removed_dates.insert(cd.date);
            }
        }
    }

    /// Returns `true` if this service is active on `date`.
    pub fn is_active_on(&self, date: NaiveDate) -> bool {
        if self.removed_dates.contains(&date) {
            return false;
        }
        if self.added_dates.contains(&date) {
            return true;
        }
        if let Some(weekly) = &self.weekly {
            if date >= weekly.start_date && date <= weekly.end_date {
                return weekly.runs_on_weekday(date.weekday());
            }
        }
        false
    }
}

impl Identifiable for ServiceCalendar {
    type Id = ServiceId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weekly_pattern_every_weekday_and_service_calendar_exceptions() {
        let start = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(); // Monday
        let end = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap(); // Sunday

        let raw_cal = RawCalendar {
            id: ServiceId::new("wkday"),
            monday: true,
            tuesday: false,
            wednesday: true,
            thursday: false,
            friday: true,
            saturday: false,
            sunday: false,
            start_date: start,
            end_date: end,
        };
        let wp = WeeklyPattern::from(&raw_cal);
        assert!(wp.runs_on_weekday(Weekday::Mon));
        assert!(!wp.runs_on_weekday(Weekday::Tue));
        assert!(wp.runs_on_weekday(Weekday::Wed));
        assert!(!wp.runs_on_weekday(Weekday::Thu));
        assert!(wp.runs_on_weekday(Weekday::Fri));
        assert!(!wp.runs_on_weekday(Weekday::Sat));
        assert!(!wp.runs_on_weekday(Weekday::Sun));

        let mut cal = ServiceCalendar::new(ServiceId::new("wkday"));
        cal.weekly = Some(wp);

        let mon = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let tue = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let before = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();

        assert!(cal.is_active_on(mon));
        assert!(!cal.is_active_on(tue));
        assert!(!cal.is_active_on(before));

        // Exception::Deleted disables otherwise active Monday
        cal.add_exception(&RawCalendarDate {
            service_id: ServiceId::new("wkday"),
            date: mon,
            exception_type: Exception::Deleted,
        });
        assert!(!cal.is_active_on(mon));

        // Exception::Added enables inactive Tuesday and out-of-range date
        cal.add_exception(&RawCalendarDate {
            service_id: ServiceId::new("wkday"),
            date: tue,
            exception_type: Exception::Added,
        });
        cal.add_exception(&RawCalendarDate {
            service_id: ServiceId::new("wkday"),
            date: before,
            exception_type: Exception::Added,
        });
        assert!(cal.is_active_on(tue));
        assert!(cal.is_active_on(before));

        // Calendar defined purely via calendar_dates.txt (weekly == None)
        let mut dates_only = ServiceCalendar::new(ServiceId::new("dates_only"));
        dates_only.add_exception(&RawCalendarDate {
            service_id: ServiceId::new("dates_only"),
            date: tue,
            exception_type: Exception::Added,
        });
        assert!(dates_only.is_active_on(tue));
        assert!(!dates_only.is_active_on(mon));
    }
}
