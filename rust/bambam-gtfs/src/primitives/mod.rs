//! Shared primitive value types (time, time windows, geographic coordinates, bounding boxes, colors)
//! and serde/rkyv helpers for `bambam-gtfs`.

mod bounding_box;
mod coord;
mod error;
mod gtfs_time;
mod rkyv_helpers;
mod serde_helpers;
mod time_window;

pub use bounding_box::BoundingBox;
pub use coord::Coord;
pub use error::PrimitiveError;
pub use gtfs_time::GtfsTime;
pub use rkyv_helpers::{RkyvNaiveDate, RkyvNaiveDateSet, RkyvRgb8};
pub use time_window::TimeWindow;

pub(crate) use serde_helpers::{
    de_with_optional_float, deserialize_bool, deserialize_date, deserialize_option_date,
    deserialize_optional_color, deserialize_optional_time, serialize_bool, serialize_date,
    serialize_float_as_str, serialize_option_date, serialize_optional_color,
    serialize_optional_time,
};
