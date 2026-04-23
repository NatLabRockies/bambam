use csv::ReaderBuilder;
use serde::{self, Deserialize};
use std::{fs::File, io, path::Path};
use zip::ZipArchive;

/// a single row from routes.txt in a GTFS-Flex feed
#[derive(Debug, Deserialize)]
pub struct Route {
    /// unique route identifier
    pub route_id: String,

    /// identifier for the transit agency
    pub agency_id: Option<String>,
}

pub fn read_routes_from_flex(zip_path: &Path) -> io::Result<Option<Vec<Route>>> {
    // open the ZIP file
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    // locate routes.txt inside the ZIP
    let mut routes_name: Option<String> = None;

    for i in 0..archive.len() {
        let file_in_zip = archive.by_index(i)?;

        if file_in_zip.name().ends_with("routes.txt") {
            // do not allow multiple routes.txt files in a zip
            if routes_name.is_some() {
                eprintln!(
                    "WARNING: Multiple routes.txt found in {:?}. Skipping ZIP.",
                    zip_path
                );
                return Ok(None);
            }

            routes_name = Some(file_in_zip.name().to_string());
        }
    }

    // handle missing routes.txt
    let routes_name = match routes_name {
        Some(name) => name,
        None => {
            println!("No routes.txt found in {:?}", zip_path);
            return Ok(None);
        }
    };

    // open routes.txt as a streaming reader
    let file_in_zip = archive.by_name(&routes_name)?;

    // create a CSV reader
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(file_in_zip);

    // deserialize each row into Route
    let mut routes = Vec::new();

    for result in rdr.deserialize::<Route>() {
        let route = result.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        routes.push(route);
    }

    Ok(Some(routes))
}
