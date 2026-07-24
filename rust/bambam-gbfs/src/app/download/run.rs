use std::{
    fs::File,
    path::Path,
    sync::{Arc, Mutex},
};

use chrono::TimeDelta;
use csv::QuoteStyle;
use flate2::{Compression, write::GzEncoder};
use geozero::ToWkt;
use kdam::{Bar, BarBuilder, BarExt};
use tokio::{
    sync::Semaphore,
    time::{Duration, Instant},
};

use crate::app::download::{
    EntryPoint, GbfsVersion, gbfs_record::GbfsRecord, gbfs_v2_2, gbfs_v2_3, gbfs_v3_0,
};

const GEOMETRIES_FILENAME: &str = "edges-gbfs-geofences-enumerated.txt.gz";
const RECORDS_FILENAME: &str = "edges-gbfs-records.csv.gz";

pub async fn gbfs_download_import(
    url: &str,
    out_dir: &Path,
    version: GbfsVersion,
    overwrite: bool,
) -> Result<(), String> {
    log::info!("run_gbfs_download with url={url}, out_dir={out_dir:?}, version={version}");

    // download GBFS dataset
    let client = reqwest::Client::new();
    let gbfs = GbfsRecord::download_from_gbfs_endpoint(&client, url, version).await?;

    // process into BAMBAM-GBFS edge list format
    let mut geometries = vec![];
    let mut zone_records = vec![];
    for i in 0..gbfs.n_features() {
        let geometry = gbfs.get_feature_geometry(i)?;
        let record = gbfs.get_feature_zone_record(i)?;
        geometries.push(geometry);
        zone_records.push(record);
    }

    // write outputs
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("failure creating output directory location: {e}"))?;
    let mut geom_writer = create_writer(
        out_dir,
        GEOMETRIES_FILENAME,
        false,
        QuoteStyle::Never,
        overwrite,
    )?;
    let mut record_writer = create_writer(
        out_dir,
        RECORDS_FILENAME,
        true,
        QuoteStyle::Necessary,
        overwrite,
    )?;

    for (idx, geom) in geometries.into_iter().enumerate() {
        let wkt_string = geom
            .to_wkt()
            .map_err(|e| format!("failure converting geometry {idx} into WKT: {e}"))?;

        geom_writer
            .serialize(&wkt_string)
            .map_err(|e| format!("failure writing geometry {idx} to file: {e}"))?
    }

    for (idx, record) in zone_records.into_iter().enumerate() {
        record_writer
            .serialize(&record)
            .map_err(|e| format!("failure writing geometry {idx} to file: {e}"))?
    }

    Ok(())
}

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
pub async fn gbfs_download_old(
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
            gbfs_v3_0::run_v3_0_manifest(&client, url).await?
        }
        (GbfsVersion::V3_0, EntryPoint::Gbfs) => gbfs_v3_0::run_v3_0_gbfs(&client, url)
            .await
            .map(|g| vec![g])?,
        _ => return Err("version not supported".to_string()),
    };

    for row in result.into_iter() {
        println!("{}", serde_json::to_string_pretty(&row).unwrap_or_default());
    }

    Ok(())
}

/// designed to download from a GBFS system list file such as
/// <https://github.com/MobilityData/gbfs/blob/master/systems.csv>.
pub async fn gbfs_batch_metadata_download(
    urls: &[String],
    entry_point: EntryPoint,
    out_dir: &Path,
    parallelism: Option<usize>,
    delay_ms: Option<u64>,
) -> Result<(), String> {
    let par = parallelism.unwrap_or(1);
    let del = delay_ms.unwrap_or_default();

    let bar: Arc<Mutex<Bar>> = Arc::new(Mutex::new(
        BarBuilder::default()
            .total(urls.len())
            .desc("gbfs urls")
            .build()
            .map_err(|e| format!("error building progress bar: {e}"))?,
    ));
    let client = Arc::new(reqwest::Client::new());
    let semaphore = Arc::new(Semaphore::new(par));
    let mut next_start_at = Instant::now();
    let spacing = Duration::from_millis(del);

    log::info!(
        "starting calls to download archives via {entry_point} entry point (parallelism={par}, delay={del})"
    );
    let mut set = tokio::task::JoinSet::new();
    for url in urls.iter() {
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
            Ok(Ok(r)) => results.extend(r),
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
        super::download_metadata::UnversionedGbfsVersion::V2_2 => {
            let result = match entry_point {
                EntryPoint::Manifest => {
                    return Err("manifest entry point not supported for version 2.2".to_string());
                }
                EntryPoint::Gbfs => gbfs_v2_2::run_v2_2_gbfs(&client, &url)
                    .await
                    .map(|g| vec![g])?,
            };
            result.into_iter().map(GbfsRecord::V2_2).collect()
        }
        super::download_metadata::UnversionedGbfsVersion::V2_3 => {
            let result = match entry_point {
                EntryPoint::Manifest => gbfs_v2_3::run_v2_3_manifest(&client, &url).await?,
                EntryPoint::Gbfs => gbfs_v2_3::run_v2_3_gbfs(&client, &url)
                    .await
                    .map(|g| vec![g])?,
            };
            result.into_iter().map(GbfsRecord::V2_3).collect()
        }
        super::download_metadata::UnversionedGbfsVersion::V3_0 => {
            let result = match entry_point {
                EntryPoint::Manifest => gbfs_v3_0::run_v3_0_manifest(&client, &url).await?,
                EntryPoint::Gbfs => gbfs_v3_0::run_v3_0_gbfs(&client, &url)
                    .await
                    .map(|g| vec![g])?,
            };
            result.into_iter().map(GbfsRecord::V3_0).collect()
        }
    };

    if let Ok(mut bar) = bar.lock() {
        let _ = bar.update(1);
    }

    Ok(result)
}

/// helper function to build a filewriter for writing either .csv.gz or
/// .txt.gz files for compass datasets while respecting the user's overwrite
/// preferences and properly formatting WKT outputs.
fn create_writer(
    directory: &Path,
    filename: &str,
    has_headers: bool,
    quote_style: QuoteStyle,
    overwrite: bool,
) -> Result<csv::Writer<GzEncoder<File>>, String> {
    let filepath = directory.join(filename);
    if filepath.exists() && !overwrite {
        return Err(format!(
            "user chose overwrite=false but file {} exists",
            filepath.to_string_lossy()
        ));
    }
    let file = File::create(&filepath)
        .map_err(|e| format!("failure creating file {}: {e}", filepath.to_string_lossy()))?;
    let buffer = GzEncoder::new(file, Compression::default());
    let writer = csv::WriterBuilder::new()
        .has_headers(has_headers)
        .quote_style(quote_style)
        .from_writer(buffer);
    Ok(writer)
}
