use std::path::Path;
use clap::Parser;

mod agency;
mod calendar;
mod flex_processor;
mod stop_times;
mod trips;
mod app;

use crate::flex_processor::process_gtfs_flex_bundle;
use crate::app::{Cli, Commands};

fn main() -> std::io::Result<()> {
    // parse cli arguments
    let cli = Cli::parse();

    match cli.command {
        Commands::ProcessGtfsFlexFeeds(args) => {
            let flex_dir = Path::new(&args.flex_dir);
            let date_requested = &args.date_requested;
            let output_file = &args.output_csv;

            // check if the input directory exists
            if !flex_dir.exists() || !flex_dir.is_dir() {
                eprintln!("Error: The specified directory does not exist or is not a directory.");
                std::process::exit(1);
            }

            // process GTFS-Flex feeds
            let valid_zones = process_gtfs_flex_bundle(flex_dir, date_requested)?;

            // write valid zones CSV
            let mut writer = csv::Writer::from_path(flex_dir.join(output_file))?;
            for zone in valid_zones {
                writer.serialize(zone)?;
            }
            writer.flush()?;

            println!("Valid zones written to {}", output_file);
        }
    }

    Ok(())
}