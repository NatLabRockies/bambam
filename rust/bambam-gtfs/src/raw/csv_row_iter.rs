//! Streaming row-by-row CSV iterator (`CsvRowIter`) for $O(1)$-memory table reads.

use crate::raw::error::{CsvRowContext, RawReadError};
use serde::de::DeserializeOwned;
use std::io::Read;
use std::marker::PhantomData;

/// Streaming row-by-row iterator over a CSV table inside a [`GtfsArchive`](crate::raw::GtfsArchive).
///
/// Enables constant-memory $O(1)$ analysis of large tables like `stop_times.txt` or `shapes.txt`
/// without allocating a full `Vec<T>`.
pub struct CsvRowIter<'a, T> {
    pub(crate) csv_reader: csv::Reader<Box<dyn Read + 'a>>,
    pub(crate) headers: csv::StringRecord,
    pub(crate) rec: csv::StringRecord,
    pub(crate) file_name: String,
    pub(crate) row_number: usize,
    pub(crate) _marker: PhantomData<T>,
}

impl<'a, T: DeserializeOwned> Iterator for CsvRowIter<'a, T> {
    type Item = Result<T, RawReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.csv_reader.read_record(&mut self.rec) {
                Ok(true) => {
                    self.row_number += 1;
                    // Skip trailing or intermediate blank lines where every field is empty/whitespace
                    if self.rec.iter().all(|f| f.trim().is_empty()) {
                        continue;
                    }
                    let res = self.rec.deserialize(Some(&self.headers)).map_err(|source| {
                        RawReadError::Csv {
                            file_name: self.file_name.clone(),
                            source,
                            row: Some(CsvRowContext {
                                row_number: self.row_number,
                                headers: self.headers.iter().map(String::from).collect(),
                                values: self.rec.iter().map(String::from).collect(),
                            }),
                        }
                    });
                    return Some(res);
                }
                Ok(false) => return None,
                Err(source) => {
                    return Some(Err(RawReadError::Csv {
                        file_name: self.file_name.clone(),
                        source,
                        row: None,
                    }))
                }
            }
        }
    }
}
