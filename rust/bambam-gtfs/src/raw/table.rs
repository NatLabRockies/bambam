//! Self-describing table trait ([`GtfsTable`]) and composable table bundle trait ([`TableBundle`]).

use crate::raw::error::RawReadError;
use crate::raw::GtfsArchive;
use std::io::{Read, Seek};

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
/// `(A, B, ...)`, extension bundle (`FlexTables`), or full [`RawFeed`](crate::raw::RawFeed) that can be loaded
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
