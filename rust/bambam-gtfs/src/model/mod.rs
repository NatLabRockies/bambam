//! Phase 2: Relationally validated GTFS + GTFS-Flex domain model (`ValidatedFeed`).

mod agency;
mod calendar;
mod error;
#[cfg(feature = "gtfs-flex")]
pub mod flex;
mod place;
mod route;
mod shape;
mod trip;
mod validate_ops;
mod validated_feed;
mod validation_policy;

pub use agency::Agency;
pub use calendar::{ServiceCalendar, WeeklyPattern};
pub use error::ModelError;
#[cfg(feature = "gtfs-flex")]
pub use flex::{BookingNotice, BookingRule, FlexDurationParams};
#[cfg(feature = "gtfs-flex")]
pub use place::{FlexLocation, LocationGroup};
pub use place::{Pathway, PlaceMember, Stop, StopArea, StopPlace, StopTransfer};
pub use route::Route;
pub use shape::{ShapeLinearRef, ShapePolyline, ShapeVertex};
pub use trip::{BoardingPolicy, FrequencyWindow, StopTime, StopTimeTiming, Trip};
pub use validated_feed::ValidatedFeed;
pub use validation_policy::{DroppedEntityCounts, ValidationPolicy, ValidationWarning};
