//! Validated `Route` model.

use crate::enums::{ContinuousPickupDropOff, RouteType};
use crate::ids::{AgencyId, RouteId};
use crate::raw::RawRoute;
use crate::traits::Identifiable;
use rgb::RGB8;

/// A validated commercial transit line (`routes.txt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    /// Unique identifier for the route.
    pub id: RouteId,
    /// Optional short name (e.g. "32", "Red").
    pub short_name: Option<String>,
    /// Optional full descriptive name.
    pub long_name: Option<String>,
    /// Optional route description.
    pub desc: Option<String>,
    /// Mode of transportation used on the route.
    pub route_type: RouteType,
    /// Optional URL for the route.
    pub url: Option<String>,
    /// Optional owning agency identifier.
    pub agency_id: Option<AgencyId>,
    /// Optional presentation sort order.
    pub order: Option<u32>,
    /// Route display color (defaults to white `#FFFFFF` when omitted in GTFS).
    pub color: RGB8,
    /// Route text color (defaults to black `#000000` when omitted in GTFS).
    pub text_color: RGB8,
    /// Continuous pickup policy along the route.
    pub continuous_pickup: ContinuousPickupDropOff,
    /// Continuous drop-off policy along the route.
    pub continuous_drop_off: ContinuousPickupDropOff,
}

impl From<RawRoute> for Route {
    fn from(raw: RawRoute) -> Self {
        Self {
            id: raw.id,
            short_name: raw.short_name,
            long_name: raw.long_name,
            desc: raw.desc,
            route_type: raw.route_type,
            url: raw.url,
            agency_id: raw.agency_id.filter(|id| !id.is_empty()),
            order: raw.order,
            color: raw.color.unwrap_or(RGB8::new(255, 255, 255)),
            text_color: raw.text_color.unwrap_or(RGB8::new(0, 0, 0)),
            continuous_pickup: raw.continuous_pickup,
            continuous_drop_off: raw.continuous_drop_off,
        }
    }
}

impl Identifiable for Route {
    type Id = RouteId;

    #[inline]
    fn id(&self) -> &Self::Id {
        &self.id
    }
}
