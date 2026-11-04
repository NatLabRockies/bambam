//! Phase 1 (`raw`) errors when reading or parsing GTFS archives and files.

use thiserror::Error;

/// Details of a specific CSV row that failed deserialization.
#[derive(Debug, Clone)]
pub struct CsvRowContext {
    /// 1-based record index in the CSV file (excluding header).
    pub row_number: usize,
    /// Column headers of the CSV file.
    pub headers: Vec<String>,
    /// Raw field values of the failing row.
    pub values: Vec<String>,
}

/// Errors occurring during Phase 1 (reading raw GTFS files from disk or zip archives).
///
/// Notice that `std::io::Error`, `csv::Error`, and `serde_json::Error` do **not** use
/// `#[from]` when a `file_name` or row context is required.
#[derive(Error, Debug)]
pub enum RawReadError {
    /// Path is neither a regular file nor a directory.
    #[error("path '{path}' is neither a file nor a directory")]
    InvalidSourcePath {
        /// The invalid path string.
        path: String,
    },

    /// A mandatory GTFS file is missing from the directory or zip archive.
    #[error("missing mandatory GTFS file '{file_name}'")]
    MissingMandatoryFile {
        /// Name of the missing file (e.g. `stops.txt`).
        file_name: String,
    },

    /// I/O error associated with a specific named file.
    #[error("I/O error while reading '{file_name}': {source}")]
    FileIo {
        /// Name or path of the file being read.
        file_name: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Error reading a zip entry for a specific named file.
    #[error("zip error while reading '{file_name}': {source}")]
    NamedZipEntry {
        /// Name of the file inside the archive.
        file_name: String,
        /// Underlying zip error.
        #[source]
        source: zip::result::ZipError,
    },

    /// Top-level zip archive format error (before a specific file is selected).
    #[error("failed to open zip archive: {0}")]
    ZipArchive(#[from] zip::result::ZipError),

    /// CSV parsing or deserialization error with file name and optional row context.
    #[error("CSV error in '{file_name}'{}: {source}", .row.as_ref().map(|r| format!(" at row {}", r.row_number)).unwrap_or_default())]
    Csv {
        /// File name being parsed (e.g. `stop_times.txt`).
        file_name: String,
        /// Underlying CSV error.
        #[source]
        source: csv::Error,
        /// Optional row headers and values where the error occurred.
        row: Option<CsvRowContext>,
    },

    /// GeoJSON syntax or schema error in `locations.geojson` (GTFS-Flex).
    #[cfg(feature = "gtfs-flex")]
    #[error("invalid GeoJSON in '{file_name}': {message}")]
    GeoJson {
        /// File name (`locations.geojson`).
        file_name: String,
        /// Detailed error message.
        message: String,
    },
}
