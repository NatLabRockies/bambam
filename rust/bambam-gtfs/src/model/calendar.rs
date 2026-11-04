//! Unified `ServiceCalendar` combining weekly patterns (`calendar.txt`) and date exceptions (`calendar_dates.txt`),
//! plus feed-level [`DateMappingPolicy`] resolution.

use crate::enums::Exception;
use crate::ids::ServiceId;
use crate::raw::{RawCalendar, RawCalendarDate};
use crate::traits::Identifiable;
use chrono::{Datelike, Days, NaiveDate, Weekday};
use std::collections::HashSet;

/// Default maximum search tolerance (`10 * 365 = 3650` days) for [`DateMappingPolicy::BestCase`].
pub const DEFAULT_BEST_CASE_TOLERANCE_DAYS: u64 = 10 * 365;

/// Policy for resolving a query `target_date` to an active service date (`picked_date`)
/// across a feed's [`ServiceCalendar`]s.
///
/// Resolving one representative service date per feed (rather than independently per `service_id`)
/// guarantees that only services actually operating on the chosen day are active together,
/// preventing weekend-only services or non-concurrent seasonal schedules from being superimposed
/// onto a weekday timetable.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum DateMappingPolicy {
    /// Requires the feed to have at least one active service on `target_date` itself.
    #[default]
    Exact,
    /// Searches outward from `target_date` within `±tolerance_days` for the closest date
    /// (preferring earlier dates on ties) on which at least one service in the feed is active.
    ///
    /// When `match_weekday` is `true`, only candidate dates with the same [`Weekday`] as
    /// `target_date` are considered (stepping in 7-day increments).
    Nearest {
        /// Maximum number of days before or after `target_date` to search.
        tolerance_days: u64,
        /// If `true`, only candidate dates matching `target_date.weekday()` are accepted.
        match_weekday: bool,
    },
    /// Cascading fallback policy:
    /// 1. [`DateMappingPolicy::Exact`] on `target_date`,
    /// 2. [`DateMappingPolicy::Nearest`] within `±tolerance_days` with `match_weekday = true`,
    /// 3. [`DateMappingPolicy::Nearest`] within `±tolerance_days` with `match_weekday = false`.
    BestCase {
        /// Maximum number of days before or after `target_date` to search.
        tolerance_days: u64,
    },
}

impl DateMappingPolicy {
    /// Creates a [`DateMappingPolicy::BestCase`] with [`DEFAULT_BEST_CASE_TOLERANCE_DAYS`] (`3650` days).
    #[inline]
    pub const fn best_case_default() -> Self {
        Self::BestCase {
            tolerance_days: DEFAULT_BEST_CASE_TOLERANCE_DAYS,
        }
    }

    /// Resolves `target_date` to a representative active service date (`picked_date`) across
    /// `services`, or returns `None` if no date satisfies the policy.
    ///
    /// When both `target_date - offset` and `target_date + offset` have active service at the
    /// same minimum distance, the earlier date (`target_date - offset`) is chosen deterministically.
    pub fn resolve_date<'a, I>(&self, target_date: NaiveDate, services: I) -> Option<NaiveDate>
    where
        I: IntoIterator<Item = &'a ServiceCalendar>,
    {
        let services: Vec<&ServiceCalendar> = services.into_iter().collect();
        self.resolve_from_slice(target_date, &services)
    }

    /// Resolves `target_date` to a representative active service date (`picked_date`) across
    /// a slice of [`ServiceCalendar`]s.
    pub fn resolve_space_slice(
        &self,
        target_date: NaiveDate,
        services: &[ServiceCalendar],
    ) -> Option<NaiveDate> {
        let refs: Vec<&ServiceCalendar> = services.iter().collect();
        self.resolve_from_slice(target_date, &refs)
    }

    fn resolve_from_slice(
        &self,
        target_date: NaiveDate,
        services: &[&ServiceCalendar],
    ) -> Option<NaiveDate> {
        if services.is_empty() {
            return None;
        }
        match *self {
            DateMappingPolicy::Exact => {
                if any_service_active_on(services, target_date) {
                    Some(target_date)
                } else {
                    None
                }
            }
            DateMappingPolicy::Nearest {
                tolerance_days,
                match_weekday,
            } => find_nearest_active_date(target_date, services, tolerance_days, match_weekday),
            DateMappingPolicy::BestCase { tolerance_days } => {
                find_nearest_active_date(target_date, services, tolerance_days, true).or_else(
                    || find_nearest_active_date(target_date, services, tolerance_days, false),
                )
            }
        }
    }
}

#[inline]
fn any_service_active_on(services: &[&ServiceCalendar], date: NaiveDate) -> bool {
    services.iter().any(|s| s.is_active_on(date))
}

fn find_nearest_active_date(
    target_date: NaiveDate,
    services: &[&ServiceCalendar],
    tolerance_days: u64,
    match_weekday: bool,
) -> Option<NaiveDate> {
    if any_service_active_on(services, target_date) {
        return Some(target_date);
    }

    let step = if match_weekday { 7 } else { 1 };
    let mut offset = step;
    while offset <= tolerance_days {
        if let Some(earlier) = target_date.checked_sub_days(Days::new(offset)) {
            if any_service_active_on(services, earlier) {
                return Some(earlier);
            }
        }
        if let Some(later) = target_date.checked_add_days(Days::new(offset)) {
            if any_service_active_on(services, later) {
                return Some(later);
            }
        }
        offset = match offset.checked_add(step) {
            Some(next) => next,
            None => break,
        };
    }
    None
}

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

    /// Resolves `target_date` to a representative active service date (`picked_date`) across
    /// a slice of [`ServiceCalendar`]s using `policy`.
    pub fn resolve_service_date(
        services: &[ServiceCalendar],
        target_date: NaiveDate,
        policy: DateMappingPolicy,
    ) -> Option<NaiveDate> {
        policy.resolve_space_slice(target_date, services)
    }
}

impl Identifiable for ServiceCalendar {
    type Id = ServiceId;

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

    #[test]
    fn test_date_mapping_policy_feed_level_resolution() {
        // Feed active 2026-11-02 (Mon) .. 2026-11-08 (Sun) with separate Weekday and Saturday services
        let start = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();

        let mut wkday = ServiceCalendar::new(ServiceId::new("wkday"));
        wkday.weekly = Some(WeeklyPattern::from(&RawCalendar {
            id: ServiceId::new("wkday"),
            monday: true,
            tuesday: true,
            wednesday: true,
            thursday: true,
            friday: true,
            saturday: false,
            sunday: false,
            start_date: start,
            end_date: end,
        }));

        let mut sat_only = ServiceCalendar::new(ServiceId::new("sat"));
        sat_only.weekly = Some(WeeklyPattern::from(&RawCalendar {
            id: ServiceId::new("sat"),
            monday: false,
            tuesday: false,
            wednesday: false,
            thursday: false,
            friday: false,
            saturday: true,
            sunday: false,
            start_date: start,
            end_date: end,
        }));

        let services = vec![wkday, sat_only];

        // 1. Target inside range (Wednesday 2026-11-04): resolves to 2026-11-04; sat_only is inactive!
        let wed = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        let picked_exact =
            ServiceCalendar::resolve_service_date(&services, wed, DateMappingPolicy::Exact);
        assert_eq!(picked_exact, Some(wed));
        assert!(services[0].is_active_on(wed));
        assert!(!services[1].is_active_on(wed));

        // 2. Target outside range (Wednesday 2026-11-18, 14 days later):
        //    - Exact returns None
        //    - Nearest with match_weekday = true steps by 7 days and picks Wednesday 2026-11-04 (not Saturday 2026-11-07!)
        let future_wed = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(
            ServiceCalendar::resolve_service_date(&services, future_wed, DateMappingPolicy::Exact),
            None
        );
        assert_eq!(
            ServiceCalendar::resolve_service_date(
                &services,
                future_wed,
                DateMappingPolicy::Nearest {
                    tolerance_days: 30,
                    match_weekday: true
                }
            ),
            Some(wed)
        );
        assert_eq!(
            ServiceCalendar::resolve_service_date(
                &services,
                future_wed,
                DateMappingPolicy::BestCase { tolerance_days: 30 }
            ),
            Some(wed)
        );

        // 3. Target is Sunday 2026-11-15, but feed has NO Sunday service at all:
        //    - Nearest { match_weekday: true } returns None
        //    - BestCase falls back to match_weekday: false and picks Saturday 2026-11-07 (7 days away)
        let sun = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        assert_eq!(
            ServiceCalendar::resolve_service_date(
                &services,
                sun,
                DateMappingPolicy::Nearest {
                    tolerance_days: 30,
                    match_weekday: true
                }
            ),
            None
        );
        assert_eq!(
            ServiceCalendar::resolve_service_date(
                &services,
                sun,
                DateMappingPolicy::BestCase { tolerance_days: 30 }
            ),
            Some(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap())
        );
    }
}
