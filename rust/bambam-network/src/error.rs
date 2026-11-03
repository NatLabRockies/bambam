use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum NetworkImportError {
    #[error("Invalid input: {0}")]
    InvalidUserInput(String),
    #[error("Serializing record into compass format failed: {0}")]
    SerializationError(String),
    #[error("Segment connectors vector is invalid or not specified: {0}")]
    InvalidSegmentConnectors(String),
    #[error("linear reference {0} must be in range [0, 1]")]
    InvalidLinearReference(f64),
    #[error("Invalid or empty geometry: {0}")]
    InvalidGeometry(String),
    #[error("Error writing to csv: {0}")]
    CsvWriteError(String),
    #[error("Error reading from '{path}': {message}")]
    ReadError { path: PathBuf, message: String },
    #[error("Error writing to '{path}': {message}")]
    WriteError { path: PathBuf, message: String },
    #[error("Invalid `between` vector: {0}")]
    InvalidBetweenVector(String),
    #[error("Required attribute is None: {0}")]
    MissingAttribute(String),
    #[error("{0}")]
    InternalError(String),
}
