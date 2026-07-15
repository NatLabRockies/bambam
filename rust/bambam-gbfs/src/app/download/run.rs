use std::path::Path;

use chrono::TimeDelta;

use crate::app::download::{EntryPoint, GbfsVersion, v3_ops};

/// downloads GBFS data for some duration. aggregates the resulting rows and writes them
/// to files to be consumed by BAMBAM.
///
/// # Arguments
/// * url - URL to the GBFS dataset's system-information file
/// * out_dir - output directory to write the processed GBFS data
/// * dur - how long to poll the GBFS API
///
/// # Result
/// If successful, returns nothing, otherwise an error
pub async fn run_gbfs_download(
    url: &str,
    out_dir: &Path,
    dur: &TimeDelta,
    entry_point: EntryPoint,
    version: GbfsVersion,
) -> Result<(), String> {
    let dur_secs = dur.as_seconds_f64();
    log::debug!(
        "run_gbfs_download with url={url}, out_dir={out_dir:?}, duration (seconds)={dur_secs}"
    );
    let client = reqwest::Client::new();

    let result = match (version, entry_point) {
        (GbfsVersion::V3_0, EntryPoint::Manifest) => {
            v3_ops::run_v3_0_manifest(&client, url).await?
        }
        (GbfsVersion::V3_0, EntryPoint::Gbfs) => {
            v3_ops::run_v3_0_gbfs(&client, url).await.map(|g| vec![g])?
        }
    };

    for row in result.into_iter() {
        println!("{}", serde_json::to_string_pretty(&row).unwrap_or_default());
    }

    Ok(())
}
