use csv::ReaderBuilder;
use serde::{self, Deserialize};
use std::{fs::File, io, path::Path};
use zip::ZipArchive;

/// single row from agency.txt in a GTFS-Flex feed
#[derive(Debug, Deserialize)]
pub struct Agency {
    /// unique agency identifier
    pub agency_id: Option<String>,

    /// agency URL
    pub _agency_url: Option<String>,

    /// primary language used by the agency
    pub _agency_lang: Option<String>,

    /// full agency name
    pub _agency_name: Option<String>,

    /// agency phone number
    pub _agency_phone: Option<String>,

    /// agency timezone
    pub _agency_timezone: Option<String>,

    /// URL to fare information
    pub _agency_fare_url: Option<String>,

    /// text-to-speech version of agency name
    pub _tts_agency_name: Option<String>,
}

/// read `agency.txt` from a single GTFS-Flex zip file
///
/// streams data directly from the zip
/// returns None if agency.txt is missing or duplicated
/// returns typed Agency rows on success
pub fn read_agency_from_flex(zip_path: &Path) -> io::Result<Option<Vec<Agency>>> {
    // open the zip file
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    // locate agency.txt inside the zip
    let mut agency_name: Option<String> = None;

    for i in 0..archive.len() {
        let file_in_zip = archive.by_index(i)?;

        if file_in_zip.name().ends_with("agency.txt") {
            // do not allow multiple agency.txt files
            if agency_name.is_some() {
                eprintln!(
                    "WARNING: Multiple agency.txt found in {:?}. Skipping ZIP.",
                    zip_path
                );
                return Ok(None);
            }

            agency_name = Some(file_in_zip.name().to_string());
        }
    }

    // handle missing agency.txt
    let agency_name = match agency_name {
        Some(name) => name,
        None => {
            println!("No agency.txt found in {:?}", zip_path);
            return Ok(None);
        }
    };

    // open agency.txt as streaming reader
    let file_in_zip = archive.by_name(&agency_name)?;

    // create CSV reader
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file_in_zip);

    // deserialize rows into Agency struct
    let mut agencies = Vec::new();

    for result in rdr.deserialize::<Agency>() {
        let agency = result.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        agencies.push(agency);
    }

    Ok(Some(agencies))
}
