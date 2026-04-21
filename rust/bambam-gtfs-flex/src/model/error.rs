use routee_compass_core::{algorithm::search::SearchTreeError, model::state::StateModelError};

#[derive(thiserror::Error, Debug)]
pub enum GtfsFlexError {
    #[error("gtfs flex modeling failed while interacting with the state due to: {0}")]
    StateModel(#[from] StateModelError),
    #[error("gtfs flex modeling failed while interacting with the search tree due to: {0}")]
    SearchTree(#[from] SearchTreeError),
    #[error("gtfs flex modeling failed while working with time values due to: {0}")]
    Chrono(String),
    #[error("gtfs flex modeling failed due to runtime error breaking model invariants: {0}")]
    Runtime(String),
    #[error("gtfs flex modeling failed due to internal error: {0}")]
    Internal(String),
}
