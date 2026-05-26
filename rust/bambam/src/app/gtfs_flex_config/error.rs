use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum GtfsFlexConfigError {
    #[error("failed to parse Label Model: {error}")]
    LabelModelRead { error: serde_json::Error },
    #[error("failed reading '{filepath}': {error}")]
    ReadFailure { filepath: String, error: String },
    #[error("{0}")]
    RunFailure(String),
    #[error("{0}")]
    InternalError(String),
    #[error("{msg}: {source}")]
    ConfigReadFailure {
        msg: String,
        source: config::ConfigError,
    },
}
