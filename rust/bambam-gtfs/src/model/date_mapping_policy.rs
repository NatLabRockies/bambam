//! Feed-level [`DateMappingPolicy`] for resolving a query date to an active service date across a feed's [`ServiceCalendar`]s.

use crate::model::ServiceCalendar;
use chrono::{Days, NaiveDate};

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
    /// When `match_weekday` is `true`, only candidate dates with the same [`chrono::Weekday`] as
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

fn any_service_active_on(services: &[&ServiceCalendar], date: NaiveDate) -> bool {
    services.iter().any(|s| s.is_active_on(date))
}

/// Returns the inclusive `(min_date, max_date)` bounds of all potentially active dates across `services`,
/// or `None` if no service can be active on any date.
fn service_date_bounds(services: &[&ServiceCalendar]) -> Option<(NaiveDate, NaiveDate)> {
    let mut bounds: Option<(NaiveDate, NaiveDate)> = None;
    let mut include_range = |start: NaiveDate, end: NaiveDate| {
        bounds = Some(match bounds {
            Some((min_d, max_d)) => (min_d.min(start), max_d.max(end)),
            None => (start, end),
        });
    };

    for service in services {
        if let Some(weekly) = &service.weekly {
            if weekly.weekday_mask != 0 && weekly.start_date <= weekly.end_date {
                include_range(weekly.start_date, weekly.end_date);
            }
        }
        for &date in &service.added_dates {
            if !service.removed_dates.contains(&date) {
                include_range(date, date);
            }
        }
    }

    bounds
}

fn find_nearest_active_date(
    target_date: NaiveDate,
    services: &[&ServiceCalendar],
    tolerance_days: u64,
    match_weekday: bool,
) -> Option<NaiveDate> {
    let (min_date, max_date) = service_date_bounds(services)?;

    if target_date >= min_date
        && target_date <= max_date
        && any_service_active_on(services, target_date)
    {
        return Some(target_date);
    }

    let step: u64 = if match_weekday { 7 } else { 1 };
    let initial_gap = if target_date > max_date {
        target_date.signed_duration_since(max_date).num_days() as u64
    } else if target_date < min_date {
        min_date.signed_duration_since(target_date).num_days() as u64
    } else {
        step
    };
    let Some(mut offset) = initial_gap.div_ceil(step).checked_mul(step) else {
        return None;
    };
    offset = offset.max(step);

    while offset <= tolerance_days {
        let earlier = target_date
            .checked_sub_days(Days::new(offset))
            .filter(|&d| d >= min_date);
        let later = target_date
            .checked_add_days(Days::new(offset))
            .filter(|&d| d <= max_date);

        if earlier.is_none() && later.is_none() {
            break;
        }

        if let Some(earlier) = earlier {
            if earlier <= max_date && any_service_active_on(services, earlier) {
                return Some(earlier);
            }
        }
        if let Some(later) = later {
            if later >= min_date && any_service_active_on(services, later) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ServiceId;
    use crate::model::WeeklyPattern;
    use crate::raw::RawCalendar;

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
        let picked_exact = DateMappingPolicy::Exact.resolve_date(wed, &services);
        assert_eq!(picked_exact, Some(wed));
        assert!(services[0].is_active_on(wed));
        assert!(!services[1].is_active_on(wed));

        // 2. Target outside range (Wednesday 2026-11-18, 14 days later):
        //    - Exact returns None
        //    - Nearest with match_weekday = true steps by 7 days and picks Wednesday 2026-11-04 (not Saturday 2026-11-07!)
        let future_wed = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(
            DateMappingPolicy::Exact.resolve_date(future_wed, &services),
            None
        );
        assert_eq!(
            DateMappingPolicy::Nearest {
                tolerance_days: 30,
                match_weekday: true,
            }
            .resolve_date(future_wed, &services),
            Some(wed)
        );
        assert_eq!(
            DateMappingPolicy::BestCase { tolerance_days: 30 }.resolve_date(future_wed, &services),
            Some(wed)
        );

        // 3. Target is Sunday 2026-11-15, but feed has NO Sunday service at all:
        //    - Nearest { match_weekday: true } returns None
        //    - BestCase falls back to match_weekday: false and picks Saturday 2026-11-07 (7 days away)
        let sun = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        assert_eq!(
            DateMappingPolicy::Nearest {
                tolerance_days: 30,
                match_weekday: true,
            }
            .resolve_date(sun, &services),
            None
        );
        assert_eq!(
            DateMappingPolicy::BestCase { tolerance_days: 30 }.resolve_date(sun, &services),
            Some(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap())
        );

        // 4. Nonempty calendar with no active dates and tolerance_days = u64::MAX terminates immediately
        let empty_cal = vec![ServiceCalendar::new(ServiceId::new("inactive"))];
        assert_eq!(
            DateMappingPolicy::Nearest {
                tolerance_days: u64::MAX,
                match_weekday: false,
            }
            .resolve_date(wed, &empty_cal),
            None
        );

        // 5. Calendar with bounded dates but no matching weekday terminates once bounds are exhausted
        assert_eq!(
            DateMappingPolicy::Nearest {
                tolerance_days: u64::MAX,
                match_weekday: true,
            }
            .resolve_date(sun, &services),
            None
        );
    }
}
