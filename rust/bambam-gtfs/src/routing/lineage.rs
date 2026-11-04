//! Borrowed views of globally unambiguous entity lineage (`FullyQualifiedTripId`, `FullyQualifiedRouteId`, `FullyQualifiedPlaceId`, `PlaceIdKind`).

use crate::ids::*;
use crate::model::{Agency, Route};
use chrono_tz::Tz;

/// Borrowed view of the complete, globally unambiguous lineage of a [`TripIdx`]:
/// `(FeedId, AgencyId, RouteId, TripId, Tz)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullyQualifiedTripId<'a> {
    /// Contiguous feed index.
    pub feed_idx: FeedIdx,
    /// Archive / feed identifier.
    pub feed_id: &'a FeedId,
    /// Resolved agency identifier (`Route::agency_id`, or the sole agency's `id` in single-agency feeds).
    pub agency_id: Option<&'a AgencyId>,
    /// Resolved agency name, if available.
    pub agency_name: Option<&'a str>,
    /// Resolved agency IANA [`chrono_tz::Tz`] timezone, if available.
    pub timezone: Option<Tz>,
    /// Route identifier (`routes.txt: route_id`).
    pub route_id: &'a RouteId,
    /// Trip identifier (`trips.txt: trip_id`).
    pub trip_id: &'a TripId,
}

/// Borrowed view of the globally unambiguous lineage of a [`RouteIdx`]:
/// `(FeedId, AgencyId, RouteId, Tz)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullyQualifiedRouteId<'a> {
    /// Contiguous feed index.
    pub feed_idx: FeedIdx,
    /// Archive / feed identifier.
    pub feed_id: &'a FeedId,
    /// Resolved agency identifier.
    pub agency_id: Option<&'a AgencyId>,
    /// Resolved agency IANA [`chrono_tz::Tz`] timezone, if available.
    pub timezone: Option<Tz>,
    /// Route identifier.
    pub route_id: &'a RouteId,
}

/// Kind of routable place referenced by a [`PlaceIdx`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceIdKind<'a> {
    /// Fixed stop from `stops.txt`.
    Stop(&'a StopId),
    /// Flexible GeoJSON zone from `locations.geojson` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    FlexZone(&'a LocationId),
    /// Location group from `location_groups.txt` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    LocationGroup(&'a LocationGroupId),
}

impl<'a> PlaceIdKind<'a> {
    /// Returns the raw string slice of the underlying identifier.
    #[inline]
    pub fn as_str(self) -> &'a str {
        match self {
            PlaceIdKind::Stop(id) => id.as_str(),
            #[cfg(feature = "gtfs-flex")]
            PlaceIdKind::FlexZone(id) => id.as_str(),
            #[cfg(feature = "gtfs-flex")]
            PlaceIdKind::LocationGroup(id) => id.as_str(),
        }
    }
}

/// Borrowed view of the globally unambiguous lineage of a [`PlaceIdx`]:
/// `(FeedId, StopId | LocationId | LocationGroupId, Tz)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullyQualifiedPlaceId<'a> {
    /// Contiguous feed index.
    pub feed_idx: FeedIdx,
    /// Archive / feed identifier.
    pub feed_id: &'a FeedId,
    /// Typed place identifier (`Stop`, `FlexZone`, or `LocationGroup`).
    pub place_id: PlaceIdKind<'a>,
    /// Resolved local IANA [`chrono_tz::Tz`] timezone for this place (`Stop::timezone` or feed `primary_timezone`).
    pub timezone: Option<Tz>,
}

pub(crate) fn resolve_route_agency<'a>(
    route: &'a Route,
    agencies: &'a [Agency],
) -> (Option<&'a AgencyId>, Option<&'a str>, Option<Tz>) {
    if let Some(aid) = &route.agency_id {
        if let Some(ag) = agencies.iter().find(|a| a.id.as_ref() == Some(aid)) {
            return (Some(aid), Some(ag.name.as_str()), Some(ag.timezone));
        }
        return (Some(aid), None, agencies.first().map(|a| a.timezone));
    }
    if let Some(ag) = agencies.first() {
        return (ag.id.as_ref(), Some(ag.name.as_str()), Some(ag.timezone));
    }
    (None, None, None)
}
