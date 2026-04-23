use std::{io, path::PathBuf};

use routee_compass_core::{algorithm::search::SearchTreeError, model::state::StateModelError};

#[derive(thiserror::Error, Debug)]
pub enum GtfsFlexError {
    #[error("io error on file '{path}': {error}")]
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("GTFS error for archive {path}: {error}")]
    GtfsRead {
        path: PathBuf,
        error: gtfs_structures::Error,
    },
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
    #[error("error writing {path}: {error}")]
    CsvWrite { path: PathBuf, error: csv::Error },
    #[error("error writing {path}: {error}")]
    IoWrite { path: PathBuf, error: io::Error },
}
