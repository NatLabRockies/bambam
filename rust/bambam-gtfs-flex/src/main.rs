use bambam_gtfs_flex::model::GtfsFlexError;
use clap::Parser;
use std::path::Path;

mod app;
mod flex_processor;

use crate::app::{Cli, Commands};
use crate::flex_processor::process_gtfs_flex_bundle;

fn main() -> Result<(), GtfsFlexError> {
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
            let csv_path = flex_dir.join(output_file);
            let mut writer =
                csv::Writer::from_path(&csv_path).map_err(|error| GtfsFlexError::CsvWrite {
                    path: csv_path.clone(),
                    error,
                })?;

            for zone in valid_zones {
                writer
                    .serialize(zone)
                    .map_err(|error| GtfsFlexError::CsvWrite {
                        path: csv_path.clone(),
                        error,
                    })?;
            }
            writer.flush().map_err(|error| GtfsFlexError::IoWrite {
                path: csv_path.clone(),
                error,
            })?;

            println!("Valid zones written to {}", output_file);
        }
    }

    Ok(())
}
