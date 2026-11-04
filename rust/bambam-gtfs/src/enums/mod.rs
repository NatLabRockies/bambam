//! Enumerations for GTFS Schedule and GTFS-Flex fields.

mod fare_enums;
#[cfg(feature = "gtfs-flex")]
mod flex_enums;
mod route_type;
mod stop_enums;
mod trip_enums;

pub use fare_enums::{DefaultFareCategory, FareMediaType, PaymentMethod, TicketingType, Transfers};
#[cfg(feature = "gtfs-flex")]
pub use flex_enums::BookingType;
pub use route_type::{ExtendedRouteType, RkyvExtendedRouteType, RouteType};
pub use stop_enums::{
    Availability, LocationType, ObjectType, PathwayDirectionType, PathwayMode, TransferType,
};
pub use trip_enums::{
    BikesAllowedType, CarsAllowedType, ContinuousPickupDropOff, DirectionType, ExactTimes,
    Exception, PickupDropOffType, TimepointType,
};

use serde::ser::Serializer;

pub(crate) fn serialize_i16_as_str<S: Serializer>(s: S, value: i16) -> Result<S::Ok, S::Error> {
    s.serialize_str(&value.to_string())
}
