use chrono::NaiveTime;
use csv::ReaderBuilder;
use serde::{self, Deserialize, Deserializer};
use std::{fs::File, io, path::Path};
use zip::ZipArchive;

/// a single row from `stop_times.txt` in a GTFS-Flex feed
#[derive(Debug, Deserialize)]
pub struct StopTimes {
    /// unique trip identifier
    pub trip_id: String,

    /// location (area) identifier
    pub location_id: String,

    // /// stop sequence (1, 2, 3, etc.)
    // pub stop_sequence: u32,
    /// start of pickup/dropoff window
    #[serde(deserialize_with = "deserialize_time")]
    pub start_pickup_drop_off_window: NaiveTime,

    /// end of pickup/dropoff window
    #[serde(deserialize_with = "deserialize_time")]
    pub end_pickup_drop_off_window: NaiveTime,

    /// pickup type (1 = pickup not allowed, 2 = pickup allowed)
    pub pickup_type: u8,

    /// dropoff type (1 = dropoff not allowed, 2 = dropoff allowed)
    pub drop_off_type: u8,
}

/// deserialize HH:MM:SS string into NaiveTime
fn deserialize_time<'de, D>(deserializer: D) -> Result<NaiveTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    NaiveTime::parse_from_str(&s, "%H:%M:%S").map_err(serde::de::Error::custom)
}

/// read stop_times.txt from a single GTFS-Flex ZIP file
///
/// streams data directly from the ZIP
/// returns None if stop_times.txt is missing or duplicated
/// returns typed StopTimes rows on success
pub fn read_stop_times_from_flex(zip_path: &Path) -> io::Result<Option<Vec<StopTimes>>> {
    // open the ZIP file
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    // locate stop_times.txt inside the ZIP
    let mut stop_times_name: Option<String> = None;

    for i in 0..archive.len() {
        let file_in_zip = archive.by_index(i)?;

        if file_in_zip.name().ends_with("stop_times.txt") {
            // do not allow multiple stop_times.txt files in a zip
            if stop_times_name.is_some() {
                eprintln!(
                    "WARNING: Multiple stop_times.txt found in {:?}. Skipping ZIP.",
                    zip_path
                );
                return Ok(None);
            }

            stop_times_name = Some(file_in_zip.name().to_string());
        }
    }

    // handle missing stop_times.txt
    let stop_times_name = match stop_times_name {
        Some(name) => name,
        None => {
            println!("No stop_times.txt found in {:?}", zip_path);
            return Ok(None);
        }
    };

    // open stop_times.txt as a streaming reader
    let file_in_zip = archive.by_name(&stop_times_name)?;

    // create a CSV reader
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file_in_zip);

    // deserialize each row into StopTimes
    let mut stop_times = Vec::new();

    for result in rdr.deserialize::<StopTimes>() {
        let stop_time = result.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        stop_times.push(stop_time);
    }

    Ok(Some(stop_times))
}
