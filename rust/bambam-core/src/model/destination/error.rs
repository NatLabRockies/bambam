use routee_compass_core::model::state::StateModelError;

use crate::model::destination::{filter::DestinationPredicate, BinRange};

#[derive(thiserror::Error, Debug)]
pub enum DestinationError {
    #[error("while testing {predicate}, {error}")]
    StateErrorInPredicate {
        predicate: DestinationPredicate,
        error: StateModelError,
    },
    #[error("while testing {bin}, {error}")]
    StateErrorInBin {
        bin: BinRange,
        error: StateModelError,
    },
}
