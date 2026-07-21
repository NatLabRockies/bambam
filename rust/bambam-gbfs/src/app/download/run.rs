use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use chrono::TimeDelta;
use kdam::{Bar, BarBuilder, BarExt};
use tokio::{
    sync::Semaphore,
    time::{Duration, Instant},
};

use crate::app::download::{EntryPoint, GbfsVersion, gbfs_record::GbfsRecord, gbfs_v2_3, gbfs_v3};

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
pub async fn run_gbfs_download_old(
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
            gbfs_v3::run_v3_0_manifest(&client, url).await?
        }
        (GbfsVersion::V3_0, EntryPoint::Gbfs) => gbfs_v3::run_v3_0_gbfs(&client, url)
            .await
            .map(|g| vec![g])?,
    };

    for row in result.into_iter() {
        println!("{}", serde_json::to_string_pretty(&row).unwrap_or_default());
    }

    Ok(())
}

/// designed to download from a GBFS system list file such as
/// <https://github.com/MobilityData/gbfs/blob/master/systems.csv>.
pub async fn run_gbfs_batch_download(
    urls: &[String],
    entry_point: EntryPoint,
    out_dir: &Path,
) -> Result<(), String> {
    // Keep defaults conservative to avoid API throttling; allow overrides via env vars.
    let max_concurrency = std::env::var("BAMBAM_GBFS_MAX_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(4);
    let request_spacing_secs = std::env::var("BAMBAM_GBFS_REQUEST_SPACING_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(2);

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .total(urls.len())
            .desc("gbfs urls")
            .build()
            .map_err(|e| format!("error building progress bar: {e}"))?,
    ));
    let client = Arc::new(reqwest::Client::new());
    let semaphore = Arc::new(Semaphore::new(max_concurrency));
    let mut next_start_at = Instant::now();
    let spacing = Duration::from_secs(request_spacing_secs);

    log::info!(
        "starting calls to download archives via {entry_point} entry point (max_concurrency={max_concurrency}, request_spacing_secs={request_spacing_secs})"
    );
    let mut set = tokio::task::JoinSet::new();
    for url in urls.into_iter() {
        let client = client.clone();
        let semaphore = semaphore.clone();
        let url = url.to_string();
        let inner_bar = bar.clone();
        let start_delay = next_start_at.saturating_duration_since(Instant::now());
        next_start_at += spacing;

        set.spawn(async move {
            if !start_delay.is_zero() {
                tokio::time::sleep(start_delay).await;
            }

            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|e| format!("failed to acquire concurrency permit: {e}"))?;

            run_gbfs_download(client, url, entry_point, inner_bar).await
        });
    }

    let mut results = vec![];
    let mut errors: Vec<String> = vec![];
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("error on tokio join of task: {e}")),
            Ok(Ok(r)) => results.extend(r.into_iter()),
        }
    }

    for result in results.into_iter() {
        let no_features = result.no_geofence();
        if !no_features {
            let filename = result.system_id();
            let filepath = out_dir.join(&filename);

            std::fs::write(
                filepath,
                serde_json::to_string_pretty(&result).unwrap_or_default(),
            )
            .map_err(|e| format!("failure while writing to '{filename}': {e}"))?;
        }
    }

    if !errors.is_empty() {
        for err in errors.iter() {
            log::error!("{err}")
        }
        log::error!("{} calls failed", errors.len());
    }

    Ok(())
}

async fn run_gbfs_download(
    client: Arc<reqwest::Client>,
    url: String,
    entry_point: EntryPoint,
    bar: Arc<Mutex<Bar>>,
) -> Result<Vec<GbfsRecord>, String> {
    let unversioned: super::download_metadata::UnversionedGbfsMetadata =
        super::ops::retrieve_file(&client, &url).await?;

    let result: Vec<GbfsRecord> = match unversioned.version {
        super::download_metadata::UnversionedGbfsVersion::V2_3 => {
            let result = match entry_point {
                EntryPoint::Manifest => gbfs_v2_3::run_v2_3_manifest(&client, &url).await?,
                EntryPoint::Gbfs => gbfs_v2_3::run_v2_3_gbfs(&client, &url)
                    .await
                    .map(|g| vec![g])?,
            };
            result.into_iter().map(|r| GbfsRecord::V2_3(r)).collect()
        }
        super::download_metadata::UnversionedGbfsVersion::V3_0 => {
            let result = match entry_point {
                EntryPoint::Manifest => gbfs_v3::run_v3_0_manifest(&client, &url).await?,
                EntryPoint::Gbfs => gbfs_v3::run_v3_0_gbfs(&client, &url)
                    .await
                    .map(|g| vec![g])?,
            };
            result.into_iter().map(|r| GbfsRecord::V3_0(r)).collect()
        }
    };

    if let Ok(mut bar) = bar.lock() {
        let _ = bar.update(1);
    }

    Ok(result)
}
