use std::path::Path;

use clap::{Parser, Subcommand};

use crate::{app::import_dataset::process_gtfs_flex_bundle, model::GtfsFlexError};

/// command line tool providing GTFS-Flex processing scripts
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// process GTFS-Flex feeds
    #[command(name = "import")]
    ProcessGtfsFlexFeeds(GtfsFLexCliArguments),
}

/// imports GTFS-Flex archives from a directory into a BAMBAM-supported file
/// format for use in route planning.
#[derive(Parser, Debug)]
pub struct GtfsFLexCliArguments {
    /// directory containing GTFS-Flex feeds to process
    pub input_directory: String,

    /// location to write the resulting dataset.
    pub output_directory: String,

    /// date for which to process GTFS-Flex feeds (format: YYYYMMDD)
    pub date_requested: String,
}

impl Cli {
    /// runs the app
    pub fn run(&self) -> Result<(), GtfsFlexError> {
        match &self.command {
            Commands::ProcessGtfsFlexFeeds(args) => {
                let in_dir = Path::new(&args.input_directory);
                let out_dir = Path::new(&args.output_directory);
                let date_requested = &args.date_requested;

                // check if the input directory exists
                if !in_dir.exists() {
                    return Err(GtfsFlexError::InputDirectoryNotFound(in_dir.to_path_buf()));
                } else if !in_dir.is_dir() {
                    return Err(GtfsFlexError::PathNotADirectory(in_dir.to_path_buf()));
                }

                // create the path to the output directory if it doesn't exist
                std::fs::create_dir_all(out_dir).map_err(|e| GtfsFlexError::IoWrite {
                    path: out_dir.to_path_buf(),
                    error: e,
                })?;

                // run the app
                process_gtfs_flex_bundle(in_dir, out_dir, date_requested)
            }
        }
    }
}
