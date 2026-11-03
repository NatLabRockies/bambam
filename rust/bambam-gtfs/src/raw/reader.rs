//! Phase 1 archive container abstraction (`GtfsArchive`), self-describing table trait (`GtfsTable`),
//! and composable table bundle trait (`TableBundle`).
//!
//! This module separates **archive transport** (Directory vs. `.zip` streams, UTF-8 BOM handling,
//! row-level CSV error reporting) from **table and extension schemas**. Adding a new GTFS
//! extension (or reading a custom agency table) requires only implementing [`GtfsTable`] or
//! [`TableBundle`]—with zero changes to the archive reader.

use crate::raw::error::{CsvRowContext, RawReadError};
use crate::raw::schedule::*;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

#[cfg(feature = "gtfs-flex")]
use crate::raw::flex::*;

/// Trait implemented by any single GTFS file/table (CSV, GeoJSON, or custom format)
/// that knows its canonical filename and how to decode itself from a [`GtfsArchive`].
pub trait GtfsTable: Sized {
    /// Canonical file name inside the GTFS archive (e.g. `"stops.txt"` or `"locations.geojson"`).
    const FILE_NAME: &'static str;

    /// Whether the GTFS specification requires this file for a standard complete feed.
    const MANDATORY: bool = false;

    /// Reads and decodes all records of this table from `archive`.
    fn read_table<R: Read + Seek>(archive: &mut GtfsArchive<R>) -> Result<Vec<Self>, RawReadError>;
}

/// Declarative macro to implement [`GtfsTable`] for any standard CSV-backed GTFS record struct.
#[macro_export]
macro_rules! impl_gtfs_csv_table {
    ($ty:ty, $file_name:literal, mandatory = $mandatory:expr) => {
        impl $crate::raw::GtfsTable for $ty {
            const FILE_NAME: &'static str = $file_name;
            const MANDATORY: bool = $mandatory;

            fn read_table<R: std::io::Read + std::io::Seek>(
                archive: &mut $crate::raw::GtfsArchive<R>,
            ) -> Result<Vec<Self>, $crate::raw::RawReadError> {
                archive.read_csv_file(Self::FILE_NAME)
            }
        }
    };
}

/// Trait implemented by any single table (`Vec<T>`, `Option<Vec<T>>`), tuple of tables
/// `(A, B, ...)`, extension bundle (`FlexTables`), or full [`RawFeed`] that can be loaded
/// selectively from a [`GtfsArchive`].
pub trait TableBundle: Sized {
    /// Loads this table bundle from `archive`.
    fn from_archive<R: Read + Seek>(archive: &mut GtfsArchive<R>) -> Result<Self, RawReadError>;
}

/// Loading `Vec<T>` requires `T::FILE_NAME` to exist in the archive (returning
/// [`RawReadError::MissingMandatoryFile`] if absent).
impl<T: GtfsTable> TableBundle for Vec<T> {
    fn from_archive<R: Read + Seek>(archive: &mut GtfsArchive<R>) -> Result<Self, RawReadError> {
        if !archive.contains_file(T::FILE_NAME) {
            return Err(RawReadError::MissingMandatoryFile {
                file_name: T::FILE_NAME.to_owned(),
            });
        }
        T::read_table(archive)
    }
}

/// Loading `Option<Vec<T>>` returns `Ok(None)` if `T::FILE_NAME` is absent from the archive,
/// or `Ok(Some(Vec<T>))` if present.
impl<T: GtfsTable> TableBundle for Option<Vec<T>> {
    fn from_archive<R: Read + Seek>(archive: &mut GtfsArchive<R>) -> Result<Self, RawReadError> {
        if !archive.contains_file(T::FILE_NAME) {
            return Ok(None);
        }
        T::read_table(archive).map(Some)
    }
}

macro_rules! impl_table_bundle_tuple {
    ($($name:ident),+) => {
        impl<$($name: TableBundle),+> TableBundle for ($($name,)+) {
            fn from_archive<R: Read + Seek>(
                archive: &mut GtfsArchive<R>,
            ) -> Result<Self, RawReadError> {
                Ok((
                    $($name::from_archive(archive)?,)+
                ))
            }
        }
    };
}

impl_table_bundle_tuple!(A, B);
impl_table_bundle_tuple!(A, B, C);
impl_table_bundle_tuple!(A, B, C, D);
impl_table_bundle_tuple!(A, B, C, D, E);
impl_table_bundle_tuple!(A, B, C, D, E, F);

/// Streaming row-by-row iterator over a CSV table inside a [`GtfsArchive`].
///
/// Enables constant-memory $O(1)$ analysis of large tables like `stop_times.txt` or `shapes.txt`
/// without allocating a full `Vec<T>`.
pub struct CsvRowIter<'a, T> {
    csv_reader: csv::Reader<Box<dyn Read + 'a>>,
    headers: csv::StringRecord,
    rec: csv::StringRecord,
    file_name: String,
    row_number: usize,
    _marker: PhantomData<T>,
}

impl<'a, T: DeserializeOwned> Iterator for CsvRowIter<'a, T> {
    type Item = Result<T, RawReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.csv_reader.read_record(&mut self.rec) {
            Ok(true) => {
                self.row_number += 1;
                let res =
                    self.rec
                        .deserialize(Some(&self.headers))
                        .map_err(|source| RawReadError::Csv {
                            file_name: self.file_name.clone(),
                            source,
                            row: Some(CsvRowContext {
                                row_number: self.row_number,
                                headers: self.headers.iter().map(String::from).collect(),
                                values: self.rec.iter().map(String::from).collect(),
                            }),
                        });
                Some(res)
            }
            Ok(false) => None,
            Err(source) => Some(Err(RawReadError::Csv {
                file_name: self.file_name.clone(),
                source,
                row: None,
            })),
        }
    }
}

enum ArchiveBackend<R> {
    Directory {
        files: HashMap<String, PathBuf>,
    },
    Zip {
        archive: zip::ZipArchive<R>,
        files: HashMap<String, usize>,
    },
}

/// Unified container handle over a GTFS directory or seekable `.zip` archive.
///
/// Supports:
/// - Type-driven selective loading via [`GtfsArchive::load`] (single tables, optional tables, tuples, or extension bundles),
/// - Constant-memory streaming via [`GtfsArchive::csv_iter`] / [`GtfsArchive::table_iter`], and
/// - Custom raw file access via [`GtfsArchive::read_csv_file`] and [`GtfsArchive::read_file_to_string`].
pub struct GtfsArchive<R = File> {
    backend: ArchiveBackend<R>,
    trim_fields: bool,
}

impl GtfsArchive<File> {
    /// Opens a GTFS directory or `.zip` file at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RawReadError> {
        let p = path.as_ref();
        if p.is_dir() {
            let mut files = HashMap::new();
            let entries = std::fs::read_dir(p).map_err(|source| RawReadError::FileIo {
                file_name: p.display().to_string(),
                source,
            })?;
            for entry in entries.flatten() {
                let ep = entry.path();
                if ep.is_file() {
                    if let Some(fname) = ep.file_name().and_then(|f| f.to_str()) {
                        if !fname.starts_with("._") {
                            files.insert(fname.to_owned(), ep);
                        }
                    }
                }
            }
            Ok(Self {
                backend: ArchiveBackend::Directory { files },
                trim_fields: true,
            })
        } else if p.is_file() {
            let file = File::open(p).map_err(|source| RawReadError::FileIo {
                file_name: p.display().to_string(),
                source,
            })?;
            Self::from_reader(file)
        } else {
            Err(RawReadError::InvalidSourcePath {
                path: p.display().to_string(),
            })
        }
    }
}

impl<R: Read + Seek> GtfsArchive<R> {
    /// Opens a seekable `.zip` archive reader as a [`GtfsArchive`].
    pub fn from_reader(reader: R) -> Result<Self, RawReadError> {
        let mut archive = zip::ZipArchive::new(reader)?;
        let mut files = HashMap::new();

        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let entry_path = Path::new(entry.name());
            if entry_path.components().any(|c| c.as_os_str() == "__MACOSX") {
                continue;
            }
            if let Some(fname) = entry_path.file_name().and_then(|f| f.to_str()) {
                if !fname.starts_with("._") {
                    files.insert(fname.to_owned(), i);
                }
            }
        }

        Ok(Self {
            backend: ArchiveBackend::Zip { archive, files },
            trim_fields: true,
        })
    }

    /// Sets whether leading/trailing whitespace should be trimmed from CSV fields (default `true`).
    pub fn with_trim_fields(mut self, trim: bool) -> Self {
        self.trim_fields = trim;
        self
    }

    /// Returns `true` if a file with `file_name` exists in the archive.
    pub fn contains_file(&self, file_name: &str) -> bool {
        match &self.backend {
            ArchiveBackend::Directory { files } => files.contains_key(file_name),
            ArchiveBackend::Zip { files, .. } => files.contains_key(file_name),
        }
    }

    /// Returns an iterator over all discovered file names in the archive.
    pub fn file_names(&self) -> impl Iterator<Item = &str> {
        let iter: Box<dyn Iterator<Item = &str>> = match &self.backend {
            ArchiveBackend::Directory { files } => Box::new(files.keys().map(String::as_str)),
            ArchiveBackend::Zip { files, .. } => Box::new(files.keys().map(String::as_str)),
        };
        iter
    }

    /// Loads any [`TableBundle`] (a single `Vec<T>`, `Option<Vec<T>>`, a tuple of tables,
    /// an extension bundle like `FlexTables`, or the full [`RawFeed`]) from this archive.
    pub fn load<B: TableBundle>(&mut self) -> Result<B, RawReadError> {
        B::from_archive(self)
    }

    /// Streams records of a [`GtfsTable`] `T` row-by-row in $O(1)$ memory.
    pub fn table_iter<T: GtfsTable + DeserializeOwned>(
        &mut self,
    ) -> Result<CsvRowIter<'_, T>, RawReadError> {
        self.csv_iter::<T>(T::FILE_NAME)
    }

    /// Streams records of `file_name` row-by-row as `T` in $O(1)$ memory.
    pub fn csv_iter<T: DeserializeOwned>(
        &mut self,
        file_name: &str,
    ) -> Result<CsvRowIter<'_, T>, RawReadError> {
        let trim_fields = self.trim_fields;
        let reader = self.open_raw_stream(file_name)?;
        let chained = strip_utf8_bom(reader, file_name)?;

        let mut csv_reader = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(if trim_fields {
                csv::Trim::Fields
            } else {
                csv::Trim::None
            })
            .from_reader(chained);

        let headers = csv_reader
            .headers()
            .map_err(|source| RawReadError::Csv {
                file_name: file_name.to_owned(),
                source,
                row: None,
            })?
            .iter()
            .map(|h| h.trim())
            .collect::<csv::StringRecord>();

        Ok(CsvRowIter {
            csv_reader,
            headers,
            rec: csv::StringRecord::new(),
            file_name: file_name.to_owned(),
            row_number: 0,
            _marker: PhantomData,
        })
    }

    /// Reads and deserializes an entire CSV file `file_name` into `Vec<T>`.
    pub fn read_csv_file<T: DeserializeOwned>(
        &mut self,
        file_name: &str,
    ) -> Result<Vec<T>, RawReadError> {
        let iter = self.csv_iter::<T>(file_name)?;
        iter.collect()
    }

    /// Reads the entire contents of `file_name` into a UTF-8 `String` (used by GeoJSON or custom text extensions).
    pub fn read_file_to_string(&mut self, file_name: &str) -> Result<String, RawReadError> {
        let reader = self.open_raw_stream(file_name)?;
        let mut chained = strip_utf8_bom(reader, file_name)?;
        let mut contents = String::new();
        chained
            .read_to_string(&mut contents)
            .map_err(|source| RawReadError::FileIo {
                file_name: file_name.to_owned(),
                source,
            })?;
        Ok(contents)
    }

    fn open_raw_stream<'a>(
        &'a mut self,
        file_name: &str,
    ) -> Result<Box<dyn Read + 'a>, RawReadError> {
        match &mut self.backend {
            ArchiveBackend::Directory { files } => {
                let path =
                    files
                        .get(file_name)
                        .ok_or_else(|| RawReadError::MissingMandatoryFile {
                            file_name: file_name.to_owned(),
                        })?;
                let file = File::open(path).map_err(|source| RawReadError::FileIo {
                    file_name: file_name.to_owned(),
                    source,
                })?;
                Ok(Box::new(file))
            }
            ArchiveBackend::Zip { archive, files } => {
                let &index =
                    files
                        .get(file_name)
                        .ok_or_else(|| RawReadError::MissingMandatoryFile {
                            file_name: file_name.to_owned(),
                        })?;
                let entry =
                    archive
                        .by_index(index)
                        .map_err(|source| RawReadError::NamedZipEntry {
                            file_name: file_name.to_owned(),
                            source,
                        })?;
                Ok(Box::new(entry))
            }
        }
    }
}

fn strip_utf8_bom<'a>(
    mut reader: Box<dyn Read + 'a>,
    file_name: &str,
) -> Result<Box<dyn Read + 'a>, RawReadError> {
    let mut prefix = [0u8; 3];
    let mut read_len = 0;
    while read_len < 3 {
        match reader.read(&mut prefix[read_len..]) {
            Ok(0) => break,
            Ok(n) => read_len += n,
            Err(source) => {
                return Err(RawReadError::FileIo {
                    file_name: file_name.to_owned(),
                    source,
                })
            }
        }
    }
    if read_len == 3 && prefix == [0xef, 0xbb, 0xbf] {
        Ok(reader)
    } else {
        let cursor = std::io::Cursor::new(prefix[..read_len].to_vec());
        Ok(Box::new(cursor.chain(reader)))
    }
}

/// Configuration options for loading a full [`RawFeed`] via [`RawFeedReader`].
#[derive(Clone, Debug)]
pub struct RawFeedReader {
    /// Whether to read `stop_times.txt` (default `true`).
    pub read_stop_times: bool,
    /// Whether to read `shapes.txt` (default `true`).
    pub read_shapes: bool,
    /// Whether to trim leading/trailing whitespace from CSV fields (default `true`).
    pub trim_fields: bool,
}

impl Default for RawFeedReader {
    fn default() -> Self {
        Self {
            read_stop_times: true,
            read_shapes: true,
            trim_fields: true,
        }
    }
}

/// Phase 1: Unvalidated, 1:1 deserialized GTFS tables read from a [`GtfsArchive`].
#[derive(Clone, Debug, Default)]
pub struct RawFeed {
    /// Rows from `agency.txt` (mandatory).
    pub agencies: Vec<RawAgency>,
    /// Rows from `stops.txt` (mandatory).
    pub stops: Vec<RawStop>,
    /// Rows from `routes.txt` (mandatory).
    pub routes: Vec<RawRoute>,
    /// Rows from `trips.txt` (mandatory).
    pub trips: Vec<RawTrip>,
    /// Rows from `stop_times.txt` (mandatory).
    pub stop_times: Vec<RawStopTime>,
    /// Rows from `calendar.txt` (optional).
    pub calendar: Option<Vec<RawCalendar>>,
    /// Rows from `calendar_dates.txt` (optional).
    pub calendar_dates: Option<Vec<RawCalendarDate>>,
    /// Rows from `shapes.txt` (optional).
    pub shapes: Option<Vec<RawShapePoint>>,
    /// Rows from `frequencies.txt` (optional).
    pub frequencies: Option<Vec<RawFrequency>>,
    /// Rows from `transfers.txt` (optional).
    pub transfers: Option<Vec<RawTransfer>>,
    /// Rows from `pathways.txt` (optional).
    pub pathways: Option<Vec<RawPathway>>,
    /// Rows from `levels.txt` (optional).
    pub levels: Option<Vec<RawLevel>>,
    /// Rows from `feed_info.txt` (optional).
    pub feed_info: Option<Vec<RawFeedInfo>>,
    /// Rows from `stop_areas.txt` (optional).
    pub stop_areas: Option<Vec<RawStopArea>>,

    /// Rows from `booking_rules.txt` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub booking_rules: Option<Vec<RawBookingRule>>,
    /// Features from `locations.geojson` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub locations: Option<Vec<RawFlexLocation>>,
    /// Rows from `location_groups.txt` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub location_groups: Option<Vec<RawLocationGroup>>,
    /// Rows from `location_group_stops.txt` (GTFS-Flex, optional).
    #[cfg(feature = "gtfs-flex")]
    pub location_group_stops: Option<Vec<RawLocationGroupStop>>,
}

impl TableBundle for RawFeed {
    fn from_archive<R: Read + Seek>(archive: &mut GtfsArchive<R>) -> Result<Self, RawReadError> {
        let schedule: ScheduleTables = archive.load()?;
        #[cfg(feature = "gtfs-flex")]
        let flex: FlexTables = archive.load()?;

        Ok(Self {
            agencies: schedule.agencies,
            stops: schedule.stops,
            routes: schedule.routes,
            trips: schedule.trips,
            stop_times: schedule.stop_times,
            calendar: schedule.calendar,
            calendar_dates: schedule.calendar_dates,
            shapes: schedule.shapes,
            frequencies: schedule.frequencies,
            transfers: schedule.transfers,
            pathways: schedule.pathways,
            levels: schedule.levels,
            feed_info: schedule.feed_info,
            stop_areas: schedule.stop_areas,
            #[cfg(feature = "gtfs-flex")]
            booking_rules: flex.booking_rules,
            #[cfg(feature = "gtfs-flex")]
            locations: flex.locations,
            #[cfg(feature = "gtfs-flex")]
            location_groups: flex.location_groups,
            #[cfg(feature = "gtfs-flex")]
            location_group_stops: flex.location_group_stops,
        })
    }
}

impl RawFeed {
    /// Reads a [`RawFeed`] from a local directory or `.zip` file path using [`GtfsArchive`].
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RawReadError> {
        GtfsArchive::open(path)?.load()
    }

    /// Reads a [`RawFeed`] from any seekable reader containing a `.zip` archive.
    pub fn from_reader<R: Read + Seek>(reader: R) -> Result<Self, RawReadError> {
        GtfsArchive::from_reader(reader)?.load()
    }
}

impl RawFeedReader {
    /// Configures whether `stop_times.txt` should be read.
    pub fn read_stop_times(mut self, read: bool) -> Self {
        self.read_stop_times = read;
        self
    }

    /// Configures whether `shapes.txt` should be read.
    pub fn read_shapes(mut self, read: bool) -> Self {
        self.read_shapes = read;
        self
    }

    /// Reads a [`RawFeed`] from a local directory or `.zip` archive path.
    pub fn read_from_path(&self, path: impl AsRef<Path>) -> Result<RawFeed, RawReadError> {
        let mut archive = GtfsArchive::open(path)?.with_trim_fields(self.trim_fields);
        self.read_from_archive(&mut archive)
    }

    /// Reads a [`RawFeed`] from a seekable `.zip` archive reader.
    pub fn read_from_reader<R: Read + Seek>(&self, reader: R) -> Result<RawFeed, RawReadError> {
        let mut archive = GtfsArchive::from_reader(reader)?.with_trim_fields(self.trim_fields);
        self.read_from_archive(&mut archive)
    }

    /// Reads a [`RawFeed`] from an open [`GtfsArchive`], respecting `read_stop_times` and `read_shapes`.
    pub fn read_from_archive<R: Read + Seek>(
        &self,
        archive: &mut GtfsArchive<R>,
    ) -> Result<RawFeed, RawReadError> {
        let agencies = archive.load()?;
        let stops = archive.load()?;
        let routes = archive.load()?;
        let trips = archive.load()?;
        let stop_times = if self.read_stop_times {
            archive.load()?
        } else {
            Vec::new()
        };
        let calendar = archive.load()?;
        let calendar_dates = archive.load()?;
        let shapes = if self.read_shapes {
            archive.load()?
        } else {
            None
        };
        let frequencies = archive.load()?;
        let transfers = archive.load()?;
        let pathways = archive.load()?;
        let levels = archive.load()?;
        let feed_info = archive.load()?;
        let stop_areas = archive.load()?;

        #[cfg(feature = "gtfs-flex")]
        let flex: FlexTables = archive.load()?;

        Ok(RawFeed {
            agencies,
            stops,
            routes,
            trips,
            stop_times,
            calendar,
            calendar_dates,
            shapes,
            frequencies,
            transfers,
            pathways,
            levels,
            feed_info,
            stop_areas,
            #[cfg(feature = "gtfs-flex")]
            booking_rules: flex.booking_rules,
            #[cfg(feature = "gtfs-flex")]
            locations: flex.locations,
            #[cfg(feature = "gtfs-flex")]
            location_groups: flex.location_groups,
            #[cfg(feature = "gtfs-flex")]
            location_group_stops: flex.location_group_stops,
        })
    }
}
