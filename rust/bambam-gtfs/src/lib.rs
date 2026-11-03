//! `gtfs2`: Next-generation phased GTFS and GTFS-Flex data model and RAPTOR-style
//! routing timetable with Linear Referencing System (LRS) shape reconstruction.
//!
//! ## Phased Initialization Pipeline
//! 1. **Phase 1 ([`raw::RawFeed`])**: 1:1 syntactic deserialization of CSV and GeoJSON files
//!    from a directory or `.zip` archive with file and row-level error reporting.
//! 2. **Phase 2 ([`model::ValidatedFeed`])**: Relational foreign-key validation, namespace
//!    collision checks, sum-type normalization ([`model::StopPlace`], [`model::StopTimeTiming`]),
//!    and [`model::ShapePolyline`] linear referencing (`ShapeLinearRef`).
//! 3. **Phase 3 ([`routing::RoutingTimetable`])**: Contiguous `0..N` typed indices (`PlaceIdx`,
//!    `RouteIdx`, `PatternIdx`, `TripIdx`), intermediate stop-time interpolation, RAPTOR-style
//!    [`routing::RoutePattern`] grouping, reverse adjacency tables, and sub-segment linestring
//!    slicing for multimodal A* graph search.

#[macro_use]
extern crate serde;

pub mod enums;
pub mod error;
pub mod ids;
pub mod model;
pub mod primitives;
pub mod raw;
pub mod routing;
pub mod traits;

#[cfg(test)]
mod tests;

pub use enums::*;
pub use error::Gtfs2Error;
pub use ids::*;
pub use model::ValidatedFeed;
pub use primitives::{BoundingBox, Coord, GtfsTime, PrimitiveError, TimeWindow};
pub use raw::{CsvRowIter, GtfsArchive, GtfsTable, RawFeed, RawFeedReader, TableBundle};
pub use routing::{
    CompactStopTime, FeedMetadata, FullyQualifiedPlaceId, FullyQualifiedRouteId,
    FullyQualifiedTripId, PlaceIdKind, RoutingTimetable, RoutingTimetableBuilder,
};
pub use traits::{Identifiable, Spatial, TemporalStopTime};

use std::convert::TryFrom;
use std::path::Path;

impl ValidatedFeed {
    /// Reads and validates a [`ValidatedFeed`] (Phase 1 -> Phase 2) from a directory or `.zip` path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Gtfs2Error> {
        let raw = RawFeed::from_path(path)?;
        Ok(Self::try_from(raw)?)
    }
}

impl RoutingTimetable {
    /// Reads, validates, and indexes a [`RoutingTimetable`] (Phase 1 -> Phase 2 -> Phase 3)
    /// from a directory or `.zip` path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Gtfs2Error> {
        let raw = RawFeed::from_path(path)?;
        let validated = ValidatedFeed::try_from(raw)?;
        Ok(Self::try_from(validated)?)
    }
}
