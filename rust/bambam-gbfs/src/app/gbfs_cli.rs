use std::path::Path;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::app::download::{EntryPoint, GbfsVersion};

/// command line tool providing GBFS processing scripts
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct GbfsCliArguments {
    /// select the GBFS operation to run
    #[command(subcommand)]
    pub op: GbfsOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand)]
pub enum GbfsOperation {
    /// downloads GBFS archives from a CSV. ignores archives missing geofence data. writes
    /// each dataset JSON 3.0 object to a file in the out directory.
    BatchDownload {
        /// a file like <https://github.com/MobilityData/gbfs/blob/master/systems.csv>
        #[arg(long)]
        csv_file: String,
        /// column name for CSV column containing URLs
        #[arg(long)]
        csv_column: String,
        /// whether we are targeting a manifest.json file or gbfs.json file.
        #[arg(long)]
        entry_point: EntryPoint,
        /// output directory path.
        #[arg(short, long, default_value_t = String::from("."))]
        output_directory: String,
        #[arg(long, default_value = None)]
        parallelism: Option<usize>,
        /// delay between calls to avoid getting rejected by provider rate limits during scraping.
        #[arg(long, default_value = None)]
        delay: Option<u64>,
        /// if true, skip the Compass network edge list import
        #[arg(long)]
        no_compass: bool,
        /// if true, skip writing the raw GBFS datasets to JSON files.
        #[arg(long)]
        no_summary: bool,
        /// whether to overwrite the files if they already exist.
        #[arg(long)]
        overwrite: bool,
    },
    /// downloads a GBFS archive from its .gbfs endpoint.
    Download {
        /// a GBFS API URL
        #[arg(long)]
        gbfs_url: String,
        /// output directory path.
        #[arg(long)]
        output_directory: String,
        /// GBFS version number to download.
        #[arg(long)]
        version: GbfsVersion,
        /// whether to overwrite the files if they already exist.
        #[arg(long)]
        overwrite: bool,
    },
}

impl GbfsOperation {
    pub async fn run(&self) -> Result<(), String> {
        match self {
            GbfsOperation::BatchDownload {
                csv_file,
                csv_column,
                entry_point,
                output_directory,
                parallelism,
                delay,
                no_compass,
                no_summary,
                overwrite,
            } => {
                let urls = crate::app::download::ops::gather_feeds(csv_file, csv_column)?;
                log::info!("found {} urls", urls.len());
                crate::app::download::run::batch_download(
                    &urls,
                    *entry_point,
                    Path::new(output_directory),
                    *parallelism,
                    *delay,
                    *no_compass,
                    *no_summary,
                    *overwrite,
                )
                .await
            }
            GbfsOperation::Download {
                gbfs_url,
                output_directory,
                version,
                overwrite,
            } => {
                crate::app::download::run::download_one(
                    gbfs_url,
                    Path::new(output_directory),
                    *version,
                    *overwrite,
                )
                .await
            }
        }
    }
}

// fn parse_duration(s: &str) -> Result<chrono::TimeDelta, String> {
//     let std_duration =
//         humantime::parse_duration(s).map_err(|e| format!("Invalid duration: {e}"))?;
//     chrono::TimeDelta::from_std(std_duration).map_err(|e| format!("TimeDelta out of range: {e}"))
// }

// fn parse_version(s: &str) -> Result<GbfsVersion, String> {
//     GbfsVersion::from_str(s)
// }
