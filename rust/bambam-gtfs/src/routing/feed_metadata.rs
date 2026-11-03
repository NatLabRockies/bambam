//! Per-archive metadata, index spans, timezone, and validation diagnostics (`FeedMetadata`).

use crate::ids::{FeedId, FeedIdx};
use crate::model::{Agency, DroppedEntityCounts, ValidationWarning};
use jiff::tz::TimeZone;
use std::ops::Range;

/// Metadata and contiguous index spans for a single GTFS archive inside a merged [`crate::routing::RoutingTimetable`].
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct FeedMetadata {
    /// Contiguous index of this feed (`0..feeds.len()`).
    pub idx: FeedIdx,
    /// Unique archive identifier (e.g. `"mdb-412"` or `"rtd-denver"`).
    pub id: FeedId,
    /// Agencies defined in this archive's `agency.txt`.
    pub agencies: Vec<Agency>,
    /// Primary parsed IANA [`jiff::tz::TimeZone`] for this feed (derived from the first agency with a valid timezone).
    #[serde(
        serialize_with = "crate::model::agency::serialize_option_timezone",
        deserialize_with = "crate::model::agency::deserialize_option_timezone",
        default
    )]
    #[rkyv(with = crate::model::agency::RkyvOptionTimeZone)]
    pub primary_timezone: Option<TimeZone>,
    /// Contiguous range of [`crate::ids::PlaceIdx`]s belonging to this feed.
    pub places_span: Range<u32>,
    /// Contiguous range of [`crate::ids::RouteIdx`]s belonging to this feed.
    pub routes_span: Range<u32>,
    /// Contiguous range of [`crate::ids::PatternIdx`]s belonging to this feed.
    pub patterns_span: Range<u32>,
    /// Contiguous range of [`crate::ids::TripIdx`]s belonging to this feed.
    pub trips_span: Range<u32>,
    /// Contiguous range of [`crate::ids::ServiceIdx`]s belonging to this feed.
    pub services_span: Range<u32>,
    /// Contiguous range of [`crate::ids::ShapeIdx`]s belonging to this feed.
    pub shapes_span: Range<u32>,
    /// Structured warnings emitted during Phase 2 and Phase 3 validation for this feed.
    pub warnings: Vec<ValidationWarning>,
    /// Summary counts of dropped or repaired entities for this feed.
    pub dropped_counts: DroppedEntityCounts,
}
