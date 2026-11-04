//! Unified GTFS archive container handle (`GtfsArchive`) over directories and `.zip` streams.

use crate::raw::csv_row_iter::CsvRowIter;
use crate::raw::error::RawReadError;
use crate::raw::table::{GtfsTable, TableBundle};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

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
    /// an extension bundle like `FlexTables`, or the full [`RawFeed`](crate::raw::RawFeed)) from this archive.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{RawCalendar, RawLevel, RawRoute, RawStop, RawStopTime};
    use std::io::{Cursor, Write};
    use std::path::PathBuf;

    fn fixture_path(rel_path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path)
    }

    #[test]
    fn test_selective_table_loading_and_streaming() {
        // 1. Selective tuple loading from a zip archive (never touches stop_times.txt or shapes.txt!)
        let mut archive = GtfsArchive::open(fixture_path("fixtures/zips/gtfs.zip"))
            .expect("should open zip archive");
        assert!(archive.contains_file("routes.txt"));
        assert!(!archive.contains_file("nonexistent_table.txt"));

        let (routes, calendar, levels): (
            Vec<RawRoute>,
            Option<Vec<RawCalendar>>,
            Option<Vec<RawLevel>>,
        ) = archive
            .load()
            .expect("should selectively load tuple of required and optional tables");
        assert_eq!(routes.len(), 1);
        assert_eq!(calendar.expect("calendar.txt present").len(), 1);
        assert!(levels.is_none());

        // 2. Streaming O(1)-memory iterator over stop_times.txt
        let stop_time_count = archive
            .table_iter::<RawStopTime>()
            .expect("should open streaming CSV iterator")
            .filter(|r| r.is_ok())
            .count();
        assert_eq!(stop_time_count, 2);

        // 3. Custom user-defined table loaded via GtfsTable without modifying reader.rs
        #[derive(Debug, Deserialize)]
        struct CustomLicenseRow {
            #[serde(rename = "license_spdx_id")]
            spdx_id: Option<String>,
        }
        crate::impl_gtfs_csv_table!(CustomLicenseRow, "licenses.txt", mandatory = false);

        let mut dir_archive = GtfsArchive::open(fixture_path("fixtures/basic"))
            .expect("should open directory archive");
        let licenses: Option<Vec<CustomLicenseRow>> = dir_archive
            .load()
            .expect("should load custom extension table");
        assert!(licenses.is_some());
        assert!(!licenses.unwrap().is_empty());
        assert!(dir_archive
            .load::<Vec<CustomLicenseRow>>()
            .unwrap()
            .iter()
            .any(|l| l.spdx_id.is_some()));
    }

    #[test]
    fn test_utf8_bom_blank_lines_row_error_context_and_macosx_filtering() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();

            // macOS resource fork files that must be ignored
            zw.start_file("__MACOSX/._stops.txt", opts).unwrap();
            zw.write_all(b"garbage").unwrap();
            zw.start_file("._routes.txt", opts).unwrap();
            zw.write_all(b"garbage").unwrap();

            // stops.txt with UTF-8 BOM, intermediate blank line, trailing blank line, and a malformed row
            zw.start_file("stops.txt", opts).unwrap();
            zw.write_all(b"\xEF\xBB\xBFstop_id,stop_name,stop_lat,stop_lon\n")
                .unwrap();
            zw.write_all(b"s1,First Stop,40.0,-105.0\n").unwrap();
            zw.write_all(b"   ,  , , \n").unwrap(); // row 2: blank/whitespace line (skipped)
            zw.write_all(b"s2,Bad Lat Stop,not_a_float,-105.0\n")
                .unwrap(); // row 3: invalid float
            zw.write_all(b"\n").unwrap();
            zw.finish().unwrap();
        }
        buf.set_position(0);

        let mut archive = GtfsArchive::from_reader(buf).expect("open in-memory zip");
        let names: Vec<&str> = archive.file_names().collect();
        assert_eq!(names, vec!["stops.txt"]);

        let mut iter = archive.table_iter::<RawStop>().expect("open stops.txt iter");
        let first = iter.next().expect("row 1").expect("row 1 should parse");
        assert_eq!(first.id.as_str(), "s1");

        let second_err = iter
            .next()
            .expect("row 3 (after skipping blank row 2)")
            .expect_err("row 3 should fail float parse");
        match second_err {
            RawReadError::Csv {
                file_name,
                row: Some(ctx),
                ..
            } => {
                assert_eq!(file_name, "stops.txt");
                assert_eq!(ctx.row_number, 3);
                assert_eq!(
                    ctx.headers,
                    vec!["stop_id", "stop_name", "stop_lat", "stop_lon"]
                );
                assert_eq!(ctx.values[0], "s2");
                assert_eq!(ctx.values[2], "not_a_float");
            }
            other => panic!("expected RawReadError::Csv with row context, got {other:?}"),
        }

        // Trailing blank line is skipped cleanly
        assert!(iter.next().is_none());
    }
}

