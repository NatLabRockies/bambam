//! Validated GTFS-Flex domain types: [`BookingRule`], [`BookingNotice`], and [`FlexDurationParams`].

use crate::enums::BookingType;
use crate::ids::{BookingRuleId, ServiceId};
use crate::primitives::GtfsTime;
use crate::raw::RawBookingRule;
use crate::traits::Identifiable;

/// Type-safe representation of when a rider must book an on-demand trip (`booking_rules.txt`).
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum BookingNotice {
    /// Real-time booking (`booking_type = 0`).
    RealTime,
    /// Same-day booking with advance notice in minutes (`booking_type = 1`).
    SameDay {
        /// Minimum minutes before travel to make the booking request.
        min_minutes: Option<i32>,
        /// Maximum minutes before travel to make the booking request.
        max_minutes: Option<i32>,
    },
    /// Prior-day(s) booking (`booking_type = 2`).
    PriorDays {
        /// Last day before travel to make the booking request.
        last_day: Option<i32>,
        /// Last time on the last day before travel.
        last_time: Option<GtfsTime>,
        /// Earliest day before travel to make the booking request.
        start_day: Option<i32>,
        /// Earliest time on the earliest day before travel.
        start_time: Option<GtfsTime>,
        /// Optional service calendar used for counting prior notice days.
        service_id: Option<ServiceId>,
    },
    /// Unrecognized integer booking type preserved from raw CSV.
    Unknown(i16),
}

/// A validated booking rule from `booking_rules.txt` (GTFS-Flex).
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct BookingRule {
    /// Unique identifier for the booking rule.
    pub id: BookingRuleId,
    /// Structured booking notice policy.
    pub notice: BookingNotice,
    /// General rider message for booking.
    pub message: Option<String>,
    /// Pickup-specific rider message.
    pub pickup_message: Option<String>,
    /// Drop-off-specific rider message.
    pub drop_off_message: Option<String>,
    /// Phone number to call for booking.
    pub phone_number: Option<String>,
    /// URL with information about the booking rule.
    pub info_url: Option<String>,
    /// URL for online booking.
    pub booking_url: Option<String>,
}

impl From<RawBookingRule> for BookingRule {
    fn from(raw: RawBookingRule) -> Self {
        let notice = match raw.booking_type {
            BookingType::RealTime => BookingNotice::RealTime,
            BookingType::SameDayWithAdvanceNotice => BookingNotice::SameDay {
                min_minutes: raw.prior_notice_duration_min,
                max_minutes: raw.prior_notice_duration_max,
            },
            BookingType::PriorDays => BookingNotice::PriorDays {
                last_day: raw.prior_notice_last_day,
                last_time: raw.prior_notice_last_time,
                start_day: raw.prior_notice_start_day,
                start_time: raw.prior_notice_start_time,
                service_id: raw.prior_notice_service_id.filter(|id| !id.is_empty()),
            },
            BookingType::Unknown(code) => BookingNotice::Unknown(code),
        };
        Self {
            id: raw.id,
            notice,
            message: raw.message,
            pickup_message: raw.pickup_message,
            drop_off_message: raw.drop_off_message,
            phone_number: raw.phone_number,
            info_url: raw.info_url,
            booking_url: raw.booking_url,
        }
    }
}

impl Identifiable for BookingRule {
    type Id = BookingRuleId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}

/// Optional duration scaling and offset parameters on a GTFS-Flex `StopTime`
/// used by routers to adjust travel time estimates inside demand-responsive zones.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct FlexDurationParams {
    /// Factor applied to base travel duration to estimate mean travel time.
    pub mean_duration_factor: Option<f64>,
    /// Offset in minutes added to estimate mean travel time.
    pub mean_duration_offset: Option<f64>,
    /// Factor applied to base travel duration to estimate 95th-percentile safe travel time.
    pub safe_duration_factor: Option<f64>,
    /// Offset in minutes added to estimate 95th-percentile safe travel time.
    pub safe_duration_offset: Option<f64>,
}

impl FlexDurationParams {
    /// Returns `true` if any duration parameter is set.
    #[inline]
    pub const fn is_some(&self) -> bool {
        self.mean_duration_factor.is_some()
            || self.mean_duration_offset.is_some()
            || self.safe_duration_factor.is_some()
            || self.safe_duration_offset.is_some()
    }

    /// Computes adjusted mean travel duration in seconds given a base street travel time in seconds.
    #[inline]
    pub fn estimate_mean_seconds(&self, base_seconds: f64) -> f64 {
        let factor = self.mean_duration_factor.unwrap_or(1.0);
        let offset_secs = self.mean_duration_offset.unwrap_or(0.0) * 60.0;
        (base_seconds * factor + offset_secs).max(0.0)
    }

    /// Computes adjusted 95th-percentile safe travel duration in seconds given a base street travel time in seconds.
    #[inline]
    pub fn estimate_safe_seconds(&self, base_seconds: f64) -> f64 {
        let factor = self.safe_duration_factor.unwrap_or(1.0);
        let offset_secs = self.safe_duration_offset.unwrap_or(0.0) * 60.0;
        (base_seconds * factor + offset_secs).max(0.0)
    }
}
