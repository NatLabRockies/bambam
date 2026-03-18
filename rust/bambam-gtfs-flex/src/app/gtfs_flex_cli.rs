use clap::{Parser, Subcommand};

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
    #[command(name = "process-feeds")]
    ProcessGtfsFlexFeeds(GTFSFLexCliArguments),
}

/// arguments for the process-gtfs-flex-feeds subcommand
#[derive(Parser, Debug)]
pub struct GTFSFLexCliArguments {
    /// directory containing GTFS-Flex feeds to process
    pub flex_dir: String,

    /// date for which to process GTFS-Flex feeds (format: YYYYMMDD)
    pub date_requested: String,

    /// output CSV file name for valid zones
    /// file will be created in the specified GTFS-Flex directory
    #[arg(short, long, default_value = "valid-zones.csv")]
    pub output_csv: String,
}
