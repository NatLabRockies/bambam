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
    process_gtfs_flex_bundle(flex_dir, date_requested)?;

    Ok(())
}
