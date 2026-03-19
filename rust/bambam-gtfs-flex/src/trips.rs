use csv::ReaderBuilder;
use serde::{self, Deserialize};
use std::{fs::File, io, path::Path};
use zip::ZipArchive;

/// s single row from trips.txt in a GTFS-Flex feed
#[derive(Debug, Deserialize)]
pub struct Trips {
    /// unique service identifier
    pub service_id: String,

    /// unique trip identifier
    pub trip_id: String,
}

/// read `trips.txt` from a single GTFS-Flex ZIP file
///
/// streams data directly from the ZIP
/// returns None if trips.txt is missing or duplicated
/// returns typed Trips rows on success
pub fn read_trips_from_flex(zip_path: &Path) -> io::Result<Option<Vec<Trips>>> {
    // open the ZIP file
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    // locate trips.txt inside the ZIP
    let mut trips_name: Option<String> = None;

    for i in 0..archive.len() {
        let file_in_zip = archive.by_index(i)?;

        if file_in_zip.name().ends_with("trips.txt") {
            // do not allow multiple trips.txt files in a zip
            if trips_name.is_some() {
                eprintln!(
                    "WARNING: Multiple trips.txt found in {:?}. Skipping ZIP.",
                    zip_path
                );
                return Ok(None);
            }

            trips_name = Some(file_in_zip.name().to_string());
        }
    }

    // handle missing trips.txt
    let trips_name = match trips_name {
        Some(name) => name,
        None => {
            println!("No trips.txt found in {:?}", zip_path);
            return Ok(None);
        }
    };

    // open trips.txt as a streaming reader
    let file_in_zip = archive.by_name(&trips_name)?;

    // create a CSV reader
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file_in_zip);

    // deserialize each row into Trips
    let mut trips = Vec::new();

    for result in rdr.deserialize::<Trips>() {
        let trip = result.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        trips.push(trip);
    }

    Ok(Some(trips))
}
