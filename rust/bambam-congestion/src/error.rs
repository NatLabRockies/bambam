use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CongestionError {
    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("I/O error on '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("CSV error in '{path}': {message}")]
    Csv { path: PathBuf, message: String },

    #[error("Parquet/Arrow error: {0}")]
    Parquet(String),

    #[error("geometry error: {0}")]
    Geometry(String),

    #[error("network dataset error: {0}")]
    Network(String),

    #[error("conflation error: {0}")]
    Conflation(String),

    #[error("spatial feature error: {0}")]
    SpatialFeature(String),

    #[error("model inference error: {0}")]
    Model(String),
}

impl From<bambam_network::NetworkImportError> for CongestionError {
    fn from(err: bambam_network::NetworkImportError) -> Self {
        Self::Network(err.to_string())
    }
}
