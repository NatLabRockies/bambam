//! Phase 1: Raw GTFS and GTFS-Flex deserialization layer.

mod archive;
mod csv_row_iter;
mod error;
#[cfg(feature = "gtfs-flex")]
pub mod flex;
mod raw_feed;
#[cfg(feature = "gtfs-flex")]
mod raw_flex_geometry;
pub mod schedule;
mod table;

pub use archive::GtfsArchive;
pub use csv_row_iter::CsvRowIter;
pub use error::{CsvRowContext, RawReadError};
#[cfg(feature = "gtfs-flex")]
pub use flex::*;
pub use raw_feed::{RawFeed, RawFeedReader};
#[cfg(feature = "gtfs-flex")]
pub use raw_flex_geometry::{RawFlexGeometry, RawFlexLocationGeometry};
pub use schedule::*;
pub use table::{GtfsTable, TableBundle};
