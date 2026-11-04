//! Phase 2 relationally validated GTFS feed (`ValidatedFeed`).

use crate::ids::*;
use crate::model::{
    Agency, DroppedEntityCounts, ModelError, Route, ServiceCalendar, ShapePolyline, Stop, StopArea,
    Trip, ValidationPolicy, ValidationWarning,
};
use crate::primitives::{Coord, GtfsTime};
use crate::raw::{RawCalendar, RawCalendarDate, RawFeed, RawFeedInfo};
use crate::traits::Spatial;
use chrono::{DateTime, NaiveDate};
use chrono_tz::Tz;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "gtfs-flex")]
use crate::model::{BookingRule, FlexLocation, LocationGroup};

/// Phase 2: Relationally validated GTFS feed where all foreign keys are resolved,
/// mutually exclusive fields are collapsed into strict enums (`StopPlace`, `StopTimeTiming`),
/// and `ShapePolyline` linear references are attached to stop times.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ValidatedFeed {
    /// Validated agencies (`agency.txt`).
    pub agencies: Vec<Agency>,
    /// Validated stops by `StopId` (`stops.txt`).
    pub stops: HashMap<StopId, Arc<Stop>>,
    /// Validated routes by `RouteId` (`routes.txt`).
    pub routes: HashMap<RouteId, Route>,
    /// Unified service calendars by `ServiceId` (`calendar.txt` + `calendar_dates.txt`).
    pub services: HashMap<ServiceId, ServiceCalendar>,
    /// Raw calendar entries by `ServiceId` (`calendar.txt`).
    pub calendar: HashMap<ServiceId, RawCalendar>,
    /// Raw calendar date exceptions by `ServiceId` (`calendar_dates.txt`).
    pub calendar_dates: HashMap<ServiceId, Vec<RawCalendarDate>>,
    /// Feed metadata rows (`feed_info.txt`).
    pub feed_info: Vec<RawFeedInfo>,
    /// Elapsed wall-clock time spent reading and validating this feed.
    pub read_duration: Duration,
    /// Validated shape polylines by `ShapeId` (`shapes.txt`).
    pub shapes: HashMap<ShapeId, Arc<ShapePolyline>>,
    /// Validated trips by `TripId` (`trips.txt` + `stop_times.txt` + `frequencies.txt`).
    pub trips: HashMap<TripId, Trip>,
    /// Validated stop areas (`stop_areas.txt`).
    pub stop_areas: Vec<StopArea>,
    /// Structured warnings recorded during validation when using [`ValidationPolicy::DropInvalidTripsWithWarning`].
    pub warnings: Vec<ValidationWarning>,
    /// Summary counts of dropped or repaired entities during validation.
    pub dropped_counts: DroppedEntityCounts,

    /// Validated booking rules by `BookingRuleId` (`booking_rules.txt`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub booking_rules: HashMap<BookingRuleId, Arc<BookingRule>>,
    /// Validated flexible GeoJSON zones by `LocationId` (`locations.geojson`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub locations: HashMap<LocationId, Arc<FlexLocation>>,
    /// Validated location groups by `LocationGroupId` (`location_groups.txt`, GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    pub location_groups: HashMap<LocationGroupId, Arc<LocationGroup>>,
}

impl ValidatedFeed {
    /// Retrieves a [`Trip`] by its string identifier.
    pub fn get_trip(&self, id: &str) -> Option<&Trip> {
        self.trips.get(id)
    }

    /// Retrieves a [`Route`] by its string identifier.
    pub fn get_route(&self, id: &str) -> Option<&Route> {
        self.routes.get(id)
    }

    /// Retrieves a [`Stop`] by its string identifier.
    pub fn get_stop(&self, id: &str) -> Option<&Arc<Stop>> {
        self.stops.get(id)
    }

    /// Retrieves a [`RawCalendar`] by its `service_id`.
    pub fn get_calendar(&self, service_id: &str) -> Option<&RawCalendar> {
        self.calendar.get(service_id)
    }

    /// Retrieves [`RawCalendarDate`] exceptions by `service_id`.
    pub fn get_calendar_date(&self, service_id: &str) -> Option<&Vec<RawCalendarDate>> {
        self.calendar_dates.get(service_id)
    }

    /// Returns the primary IANA [`chrono_tz::Tz`] timezone of this feed (from the first agency in `agency.txt`).
    #[inline]
    pub fn primary_timezone(&self) -> Option<Tz> {
        self.agencies.first().map(|a| a.timezone)
    }

    /// Resolves the [`chrono_tz::Tz`] timezone for `route_id` by matching `Route::agency_id` against
    /// `self.agencies`, falling back to [`ValidatedFeed::primary_timezone`] for single-agency feeds.
    pub fn route_timezone(&self, route_id: &str) -> Option<Tz> {
        let route = self.routes.get(route_id)?;
        if let Some(aid) = &route.agency_id {
            if let Some(ag) = self.agencies.iter().find(|a| a.id.as_ref() == Some(aid)) {
                return Some(ag.timezone);
            }
        }
        self.primary_timezone()
    }

    /// Resolves the [`chrono_tz::Tz`] timezone for `trip_id` via its route's agency.
    pub fn trip_timezone(&self, trip_id: &str) -> Option<Tz> {
        let trip = self.trips.get(trip_id)?;
        self.route_timezone(trip.route_id.as_str())
    }

    /// Resolves the [`chrono_tz::Tz`] timezone for `stop_id`, preferring `Stop::timezone` (`stop_timezone`),
    /// then any parent station's `timezone`, and falling back to [`ValidatedFeed::primary_timezone`].
    pub fn stop_timezone(&self, stop_id: &str) -> Option<Tz> {
        let stop = self.stops.get(stop_id)?;
        if let Some(tz) = stop.timezone {
            return Some(tz);
        }
        if let Some(parent_id) = &stop.parent_station {
            if let Some(parent) = self.stops.get(parent_id) {
                if let Some(tz) = parent.timezone {
                    return Some(tz);
                }
            }
        }
        self.primary_timezone()
    }

    /// Appends this archive's primary `agency_timezone` to `(service_date, time)` to construct a
    /// timezone-aware [`DateTime<Tz>`].
    pub fn resolve_datetime(
        &self,
        service_date: NaiveDate,
        time: GtfsTime,
    ) -> Option<DateTime<Tz>> {
        time.to_zoned_datetime(service_date, self.primary_timezone()?)
    }

    /// Appends the agency timezone of `trip_id` to `(service_date, time)` to construct a
    /// timezone-aware [`DateTime<Tz>`].
    pub fn resolve_trip_datetime(
        &self,
        trip_id: &str,
        service_date: NaiveDate,
        time: GtfsTime,
    ) -> Option<DateTime<Tz>> {
        let tz = self.trip_timezone(trip_id)?;
        time.to_zoned_datetime(service_date, tz)
    }

    /// Reconstructs the geographic linestring (`Vec<Coord>`) for `trip_id` between
    /// `from_stop_offset` and `to_stop_offset` (inclusive indices into `trip.stop_times`).
    ///
    /// Uses the pre-resolved [`crate::model::ShapeLinearRef`] on each [`crate::model::StopTime`] if the trip has a shape,
    /// or falls back to straight-line representative coordinates between the stops.
    pub fn reconstruct_trip_leg_linestring(
        &self,
        trip_id: &TripId,
        from_stop_offset: usize,
        to_stop_offset: usize,
    ) -> Option<Vec<Coord>> {
        let trip = self.trips.get(trip_id)?;
        if from_stop_offset >= trip.stop_times.len()
            || to_stop_offset >= trip.stop_times.len()
            || from_stop_offset > to_stop_offset
        {
            return None;
        }

        if let Some(shape_id) = &trip.shape_id {
            if let Some(shape) = self.shapes.get(shape_id) {
                let start_ref = trip.stop_times[from_stop_offset].shape_ref;
                let end_ref = trip.stop_times[to_stop_offset].shape_ref;
                if let (Some(s_ref), Some(e_ref)) = (start_ref, end_ref) {
                    return Some(shape.slice_linestring(s_ref.arc_meters, e_ref.arc_meters));
                }
            }
        }

        // Fallback: connect representative coordinates of the stop places directly
        let coords: Vec<_> = trip.stop_times[from_stop_offset..=to_stop_offset]
            .iter()
            .filter_map(|st| st.place.representative_coord())
            .collect();
        Some(coords)
    }

    /// Validates a [`RawFeed`] into a [`ValidatedFeed`] using the specified [`ValidationPolicy`].
    ///
    /// - Under [`ValidationPolicy::Strict`], returns `Err(ModelError)` on the first broken
    ///   foreign key, non-monotonic shape distance, or invalid stop-time timing.
    /// - Under [`ValidationPolicy::DropInvalidTripsWithWarning`], drops malformed trips
    ///   (including trips with 0 or 1 stops, duplicate `stop_sequence` numbers, or broken stop/timing rows)
    ///   and records structured [`ValidationWarning`]s and [`DroppedEntityCounts`].
    pub fn from_raw_with_policy(
        raw: RawFeed,
        policy: ValidationPolicy,
    ) -> Result<Self, ModelError> {
        crate::model::validate_ops::validate_raw_feed(raw, policy)
    }
}

impl TryFrom<RawFeed> for ValidatedFeed {
    type Error = ModelError;

    fn try_from(raw: RawFeed) -> Result<Self, Self::Error> {
        Self::from_raw_with_policy(raw, ValidationPolicy::Strict)
    }
}
