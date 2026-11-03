//! Top-level error aggregation for `gtfs2`.
//!
//! Each phase module (`primitives`, `raw`, `model`, `routing`) defines its own
//! `thiserror` error type, and [`Gtfs2Error`] aggregates them.

use crate::model::ModelError;
use crate::primitives::PrimitiveError;
use crate::raw::RawReadError;
use crate::routing::RoutingError;
use thiserror::Error;

/// Top-level error type for the `gtfs2` phased initialization and routing pipeline.
#[derive(Error, Debug)]
pub enum Gtfs2Error {
    /// Error parsing a primitive GTFS value (time, date, color).
    #[error(transparent)]
    Primitive(#[from] PrimitiveError),

    /// Phase 1 error reading or deserializing raw GTFS archive files.
    #[error(transparent)]
    Raw(#[from] RawReadError),

    /// Phase 2 error validating relational integrity or domain invariants.
    #[error(transparent)]
    Model(#[from] ModelError),

    /// Phase 3 error constructing the routing timetable or extracting shape sub-segments.
    #[error(transparent)]
    Routing(#[from] RoutingError),
}
