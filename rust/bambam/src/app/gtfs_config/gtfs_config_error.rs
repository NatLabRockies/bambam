#[derive(thiserror::Error, Debug)]
pub enum GtfsConfigError {
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
