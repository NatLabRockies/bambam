use std::{path::Path, str::FromStr};

use chrono::TimeDelta;
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
    /// runs a GBFS download, writing data from some source URL
    /// to an output directory.
    Download {
        /// a GBFS API URL
        #[arg(short, long)]
        gbfs_url: String,
        /// output directory path.
        #[arg(short, long, default_value_t = String::from("."))]
        output_directory: String,
        /// duration to collect data rows. provide in human-readable time values
        /// 2m, 30s, 2h, 2days... applies to wait time modeling capability.
        #[arg(short, long, value_parser = parse_duration, default_value = "10m")]
        collect_duration: TimeDelta,
        /// target of the initial HTTP call.
        #[arg(long, default_value_t = EntryPoint::Gbfs)]
        entry_point: EntryPoint,
        /// GBFS version number to download.
        #[arg(long, default_value_t = GbfsVersion::V3_0, value_parser = parse_version)]
        version: GbfsVersion,
    },
}

impl GbfsOperation {
    pub async fn run(&self) -> Result<(), String> {
        match self {
            GbfsOperation::Download {
                gbfs_url,
                output_directory,
                collect_duration,
                entry_point,
                version,
            } => {
                crate::app::download::run_gbfs_download(
                    gbfs_url,
                    Path::new(output_directory),
                    collect_duration,
                    *entry_point,
                    *version,
                )
                .await
            }
        }
    }
}

fn parse_duration(s: &str) -> Result<chrono::TimeDelta, String> {
    let std_duration =
        humantime::parse_duration(s).map_err(|e| format!("Invalid duration: {e}"))?;
    chrono::TimeDelta::from_std(std_duration).map_err(|e| format!("TimeDelta out of range: {e}"))
}

fn parse_version(s: &str) -> Result<GbfsVersion, String> {
    GbfsVersion::from_str(s)
}
