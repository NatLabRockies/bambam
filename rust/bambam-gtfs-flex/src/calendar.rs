use chrono::NaiveDate;
use csv::ReaderBuilder;
use serde::{self, Deserialize, Deserializer};
use std::{fs::File, io, path::Path};
use zip::ZipArchive;

/// a single row from calendar.txt in a GTFS-Flex feed
#[derive(Debug, Deserialize)]
pub struct Calendar {
    /// unique service identifier
    pub service_id: String,

    /// service availability by day (0 or 1)
    pub monday: u8,
    pub tuesday: u8,
    pub wednesday: u8,
    pub thursday: u8,
    pub friday: u8,
    pub saturday: u8,
    pub sunday: u8,

    /// service start date (YYYYMMDD)
    #[serde(deserialize_with = "gtfs_flex_date")]
    pub start_date: NaiveDate,

    /// service end date (YYYYMMDD)
    #[serde(deserialize_with = "gtfs_flex_date")]
    pub end_date: NaiveDate,
}

/// deserialize GTFS-Flex dates in YYYYMMDD format
fn gtfs_flex_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    NaiveDate::parse_from_str(&s, "%Y%m%d").map_err(serde::de::Error::custom)
}

/// read calendar.txt from a single GTFS-Flex ZIP file
///
/// streams data directly from the ZIP
/// returns None if calendar.txt is missing or duplicated
/// returns typed Calendar rows on success
pub fn read_calendar_from_flex(zip_path: &Path) -> io::Result<Option<Vec<Calendar>>> {
    // open the zip file
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    // locate calendar.txt
    let mut calendar_name: Option<String> = None;

    for i in 0..archive.len() {
        let file_in_zip = archive.by_index(i)?;

        if file_in_zip.name().ends_with("calendar.txt") {
            // donot allow multiple calendar.txt files in a zip
            if calendar_name.is_some() {
                eprintln!(
                    "WARNING: Multiple calendar.txt found in {:?}. Skipping ZIP.",
                    zip_path
                );
                return Ok(None);
            }

            calendar_name = Some(file_in_zip.name().to_string());
        }
    }

    // handle missing calendar.txt
    let calendar_name = match calendar_name {
        Some(name) => name,
        None => {
            println!("No calendar.txt found in {:?}", zip_path);
            return Ok(None);
        }
    };

    // open calendar.txt as a streaming reader
    let file_in_zip = archive.by_name(&calendar_name)?;

    // create a CSV reader
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file_in_zip);

    // deserialize each row into Calendar
    let mut calendars = Vec::new();

    for result in rdr.deserialize::<Calendar>() {
        let calendar = result.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        calendars.push(calendar);
    }

    Ok(Some(calendars))
}
