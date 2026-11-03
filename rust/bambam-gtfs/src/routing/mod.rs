//! Phase 3: Route-planning timetable and spatial/shape traversal structures (`RoutingTimetable`).

mod error;
mod timetable;

pub use error::RoutingError;
pub use timetable::{
    CompactStopTime, FeedMetadata, FullyQualifiedPlaceId, FullyQualifiedRouteId,
    FullyQualifiedTripId, IndexedTransfer, PatternStopProfile, PlaceIdKind, PlacePatternOccurrence,
    RoutableStopTiming, RoutableTrip, RoutePattern, RoutingTimetable, RoutingTimetableBuilder,
};
