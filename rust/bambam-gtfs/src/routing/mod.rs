//! Phase 3: Route-planning timetable and spatial/shape traversal structures (`RoutingTimetable`).

mod builder;
mod compact_stop_time;
mod departure;
mod error;
mod feed_metadata;
mod lineage;
mod pattern;
mod routable_trip;
mod spatial_index;
mod timetable;

pub use builder::RoutingTimetableBuilder;
pub use compact_stop_time::{CompactStopTime, RoutableStopTiming};
pub use departure::{ActiveServiceWindow, BoardableDeparture};
pub use error::RoutingError;
pub use feed_metadata::FeedMetadata;
pub use lineage::{
    FullyQualifiedPlaceId, FullyQualifiedRouteId, FullyQualifiedTripId, PlaceIdKind,
};
pub use pattern::{IndexedTransfer, PatternStopProfile, PlacePatternOccurrence, RoutePattern};
pub use routable_trip::RoutableTrip;
pub use spatial_index::{PlaceSpatialEntry, RkyvSpatialIndex};
pub use timetable::{ArchivedRoutingTimetable, RoutingTimetable};
