use std::path::Path;

use chrono::TimeDelta;
use gbfs_types::v3_0::files::SystemInformationFile;

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
pub async fn run_gbfs_download(url: &str, out_dir: &Path, dur: &TimeDelta) -> Result<(), String> {
    let dur_secs = dur.as_seconds_f64();
    log::debug!(
        "run_gbfs_download with url={url}, out_dir={out_dir:?}, duration (seconds)={dur_secs}"
    );
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "rust-reqwest")
        .send()
        .await
        .map_err(|e| format!("failed to connect to GBFS URL: {e}"))?;

    if response.status().is_success() {
        let system_information: SystemInformationFile = response.json().await.map_err(|e| {
            format!("failed to deserialize system information file from HTTP response: {e}")
        })?;
        println!("systemInformation version: {}", system_information.version);
        println!(
            "systemInformation data: {}",
            system_information.data.system_id
        );
    }

    Ok(())
}
