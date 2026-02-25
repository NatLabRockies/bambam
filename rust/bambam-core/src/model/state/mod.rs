pub mod fieldname;
mod multimodal_mapping;
pub mod multimodal_state_ops;
pub mod variable;

pub use multimodal_mapping::MultimodalMapping;
pub use multimodal_mapping::MultimodalStateMapping;
/// trip legs are enumerated starting from 0 to support zero-based indexing arithmetic.
pub type LegIdx = u64;

/// value for entries in a [`use routee_compass_core::model::label::Label`] denoting
/// no modes are assigned for a given trip leg.
pub const EMPTY_TRIP_LABEL: usize = usize::MAX;
