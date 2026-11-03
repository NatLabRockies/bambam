//! Phase 3: Route-planning timetable and spatial/shape traversal structures (`RoutingTimetable`).

mod error;
mod timetable;

pub use error::RoutingError;
pub use timetable::{
    ActiveServiceWindow, ArchivedRoutingTimetable, BoardableDeparture, CompactStopTime,
    FeedMetadata, FullyQualifiedPlaceId, FullyQualifiedRouteId, FullyQualifiedTripId,
    IndexedTransfer, PatternStopProfile, PlaceIdKind, PlacePatternOccurrence, PlaceSpatialEntry,
    RoutableStopTiming, RoutableTrip, RoutePattern, RoutingTimetable, RoutingTimetableBuilder,
};
