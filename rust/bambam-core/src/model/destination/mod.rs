mod error;
mod filter;
pub mod iter;
mod range;

pub use error::DestinationError;
pub use filter::{DestinationFilter, DestinationPredicate};
pub use range::{BinRange, BinRangeConfig};
