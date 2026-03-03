use std::path::Path;

mod calendar;
mod flex_processor;
mod stop_times;
mod trips;

use crate::flex_processor::process_gtfs_flex_bundle;

fn main() -> std::io::Result<()> {
    // feeds path directory
    let flex_dir = Path::new("src/test/assets");

    // requested datefor processing GTFS-Flex feeds
    let date_requested = "20240902";

    // process GTFS-Flex feeds in the specified directory for the requested date
    let valid_zones = process_gtfs_flex_bundle(flex_dir, date_requested)?;

    // write valid zones to a csv file
    let mut writer = csv::Writer::from_path(flex_dir.join("valid-zones.csv"))?;
    for zone in valid_zones {
        writer.serialize(zone)?;
    }
    writer.flush()?;

    Ok(())
}
