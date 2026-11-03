//! Phase 1: Raw GTFS and GTFS-Flex deserialization layer.

mod error;
#[cfg(feature = "gtfs-flex")]
pub mod flex;
mod reader;
pub mod schedule;

pub use error::{CsvRowContext, RawReadError};
#[cfg(feature = "gtfs-flex")]
pub use flex::*;
pub use reader::{CsvRowIter, GtfsArchive, GtfsTable, RawFeed, RawFeedReader, TableBundle};
pub use schedule::*;
