use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum ZoneError {
    #[error("failure reading file from {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse '{path}' due to: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("failed to deserialize column {col} in file '{path}' due to: {message}")]
    Deserialize {
        col: String,
        path: PathBuf,
        message: String,
    },
    #[error("failure building zonal model: {0}")]
    Build(String),
}
