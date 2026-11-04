//! Validated spatial places: [`Stop`], [`FlexLocation`], [`LocationGroup`], and the unified [`StopPlace`] enum.

#[cfg(feature = "gtfs-flex")]
mod flex_location;
#[cfg(feature = "gtfs-flex")]
mod location_group;
mod stop;
mod stop_place;

#[cfg(feature = "gtfs-flex")]
pub use flex_location::FlexLocation;
#[cfg(feature = "gtfs-flex")]
pub use location_group::LocationGroup;
pub use stop::{Pathway, Stop, StopTransfer};
pub use stop_place::{PlaceMember, StopArea, StopPlace};
