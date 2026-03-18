use routee_compass_core::model::state::StateModelError;

use crate::model::destination::{filter::DestinationPredicate, BinInterval};

#[derive(thiserror::Error, Debug)]
pub enum DestinationError {
    #[error("while testing {predicate}, {error}")]
    StateErrorInPredicate {
        predicate: DestinationPredicate,
        error: StateModelError,
    },
    #[error("while testing {bin}, {error}")]
    StateErrorInBin {
        bin: BinInterval,
        error: StateModelError,
    },
    #[error("invalid bin configuration: {reason}")]
    InvalidBinConfig { reason: String },
}
