use std::{fs::File, path::Path};

use arrow::datatypes::FieldRef;
use parquet::{arrow::ArrowWriter, basic::Compression, file::properties::WriterProperties};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_arrow::schema::{SchemaLike, TracingOptions};

use crate::{
    config::CalibrationConfig,
    conflate::{
        build_reference_rtree_in_bbox, conflate_open_network_with_reference, ConflatedEdgeSpeeds,
    },
    error::CongestionError,
    features::SpatialFeatureEngine,
    network::LoadedOpenNetwork,
};

/// A single long-format training observation (`(edge_id, time_bin)`) ready for XGBoost calibration.
///
/// Field names are serialized to match the exact predictor names in `models.py`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSampleRow {
    #[serde(rename = "ID")]
    pub source_id: String,
    pub edge_id: usize,
    pub period: String,
    #[serde(rename = "Speed Limit")]
    pub speed_limit: Option<f32>,
    #[serde(rename = "Population Density")]
    pub population_density: f32,
    #[serde(rename = "Employment Density")]
    pub employment_density: f32,
    #[serde(rename = "Day: Weekday")]
    pub day_weekday: f32,
    #[serde(rename = "Hour: AM Peak")]
    pub hour_am_peak: f32,
    #[serde(rename = "Hour: PM Peak")]
    pub hour_pm_peak: f32,
    #[serde(rename = "Road Class: Motorway")]
    pub rc_motorway: f32,
    #[serde(rename = "Road Class: Trunk")]
    pub rc_trunk: f32,
    #[serde(rename = "Road Class: Primary")]
    pub rc_primary: f32,
    #[serde(rename = "Road Class: Secondary")]
    pub rc_secondary: f32,
    #[serde(rename = "Road Class: Tertiary")]
    pub rc_tertiary: f32,
    #[serde(rename = "Road Class: Service")]
    pub rc_service: f32,
    #[serde(rename = "Road Class: Link")]
    pub rc_link: f32,
    #[serde(rename = "Road Class: Other")]
    pub rc_other: f32,
    #[serde(rename = "Opp Density")]
    pub opp_density: f32,
    #[serde(rename = "Traffic Signal/Crossing/Traffic Stop Density")]
    pub traffic_control_density: f32,
    #[serde(rename = "Speed")]
    pub observed_speed_mph: f32,
    pub overlap_meters: f32,
    pub matched_reference_count: u32,
}

/// Wide-format summary row for each conflated Open Network edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflatedEdgeSummaryRow {
    pub edge_id: usize,
    pub source_id: String,
    pub highway: String,
    pub road_class: String,
    pub length_meters: f64,
    pub posted_speed_mph: Option<f64>,
    pub imputed_speed_limit_mph: f64,
    pub pop_density_1000_per_sq_mi: f32,
    pub emp_density_1000_per_sq_mi: f32,
    pub opp_density_per_sq_mi: f32,
    pub traffic_control_density_per_sq_mi: f32,
    pub matched_reference_count: usize,
    pub total_overlap_meters: f64,
    #[serde(rename = "med-wd-am")]
    pub med_wd_am: Option<f64>,
    #[serde(rename = "med-wd-af")]
    pub med_wd_af: Option<f64>,
    #[serde(rename = "med-wd-pm")]
    pub med_wd_pm: Option<f64>,
    #[serde(rename = "med-we-am")]
    pub med_we_am: Option<f64>,
    #[serde(rename = "med-we-af")]
    pub med_we_af: Option<f64>,
    #[serde(rename = "med-we-pm")]
    pub med_we_pm: Option<f64>,
}

/// Summary statistics returned by `run_calibration`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSummary {
    pub total_open_edges: usize,
    pub high_functional_edges: usize,
    pub conflated_edges: usize,
    pub samples_with_speedlimit: usize,
    pub samples_without_speedlimit: usize,
}

/// Runs the end-to-end congestion calibration pipeline:
/// 1. Loads the Open Network (OSM/OMF) in RouteE Compass format.
/// 2. Streams and indexes Reference Network edges within the Open Network's bounding box across `config.time_bins`.
/// 3. Conflates high-functional class Open Network edges with Reference Network edges (5m corridor, $\le 45^\circ$ circular heading).
/// 4. Computes 0.5-mile spatial densities (`Opp Density`, `Traffic Control Density`, `Population Density`, `Employment Density`)
///    for all conflated edges.
/// 5. Writes `calibration_with_speedlimit.{parquet,csv}`, `calibration_without_speedlimit.{parquet,csv}`,
///    and `conflated_edges.{parquet,csv}`.
pub fn run_calibration(config: &CalibrationConfig) -> Result<CalibrationSummary, CongestionError> {
    let open_net = LoadedOpenNetwork::load(&config.open_network)?;
    let reference_rtree = build_reference_rtree_in_bbox(
        &config.reference_network,
        &config.time_bins,
        open_net.bbox,
        config.conflation.buffer_distance_meters,
    )?;

    let conflated_per_edge: Vec<Option<ConflatedEdgeSpeeds>> = conflate_open_network_with_reference(
        &open_net,
        &reference_rtree,
        config.time_bins.len(),
        &config.conflation,
    );

    let conflated_count = conflated_per_edge.iter().filter(|c| c.is_some()).count();
    let high_func_count = open_net
        .edges
        .iter()
        .filter(|e| e.road_class.is_high_functional())
        .count();
    log::info!(
        "Conflated {conflated_count} / {high_func_count} high-functional edges with reference network speeds"
    );

    let feature_engine = SpatialFeatureEngine::build(&open_net, &config.spatial_features)?;

    // Only compute spatial densities for edges that actually matched the reference network
    use rayon::prelude::*;
    let edge_features: Vec<_> = open_net
        .edges
        .par_iter()
        .zip(conflated_per_edge.par_iter())
        .map(|(edge, conf_opt)| feature_engine.compute_edge_features(edge, conf_opt.is_some()))
        .collect();

    let mut samples_with_sl: Vec<CalibrationSampleRow> = Vec::new();
    let mut samples_without_sl: Vec<CalibrationSampleRow> = Vec::new();
    let mut wide_rows: Vec<ConflatedEdgeSummaryRow> = Vec::with_capacity(conflated_count);

    for ((edge, conf_opt), feat) in open_net
        .edges
        .iter()
        .zip(conflated_per_edge.iter())
        .zip(edge_features.iter())
    {
        let Some(conf) = conf_opt else {
            continue;
        };

        let find_bin_speed = |target_name: &str| -> Option<f64> {
            config
                .time_bins
                .iter()
                .position(|b| b.name == target_name)
                .and_then(|idx| conf.speeds_mph.get(idx).copied().flatten())
        };

        wide_rows.push(ConflatedEdgeSummaryRow {
            edge_id: edge.edge_id,
            source_id: edge.source_id.clone(),
            highway: edge.highway_group.clone(),
            road_class: edge.road_class.as_str().to_string(),
            length_meters: edge.length_meters,
            posted_speed_mph: edge.posted_speed_mph,
            imputed_speed_limit_mph: edge.imputed_speed_limit_mph,
            pop_density_1000_per_sq_mi: feat.pop_density_1000_per_sq_mi,
            emp_density_1000_per_sq_mi: feat.emp_density_1000_per_sq_mi,
            opp_density_per_sq_mi: feat.opp_density_per_sq_mi,
            traffic_control_density_per_sq_mi: feat.traffic_control_density_per_sq_mi,
            matched_reference_count: conf.matched_reference_count,
            total_overlap_meters: conf.total_overlap_meters,
            med_wd_am: find_bin_speed("wd-am"),
            med_wd_af: find_bin_speed("wd-af"),
            med_wd_pm: find_bin_speed("wd-pm"),
            med_we_am: find_bin_speed("we-am"),
            med_we_af: find_bin_speed("we-af"),
            med_we_pm: find_bin_speed("we-pm"),
        });

        let rc = feat.road_class.one_hot_8();
        for (p_idx, bin) in config.time_bins.iter().enumerate() {
            let Some(obs_speed) = conf.speeds_mph.get(p_idx).copied().flatten() else {
                continue;
            };
            let (day_wd, hr_am, hr_pm) = bin.time_flags();
            let sample = CalibrationSampleRow {
                source_id: edge.source_id.clone(),
                edge_id: edge.edge_id,
                period: bin.name.clone(),
                speed_limit: feat.model_speed_limit_mph,
                population_density: feat.pop_density_1000_per_sq_mi,
                employment_density: feat.emp_density_1000_per_sq_mi,
                day_weekday: day_wd,
                hour_am_peak: hr_am,
                hour_pm_peak: hr_pm,
                rc_motorway: rc[0],
                rc_trunk: rc[1],
                rc_primary: rc[2],
                rc_secondary: rc[3],
                rc_tertiary: rc[4],
                rc_service: rc[5],
                rc_link: rc[6],
                rc_other: rc[7],
                opp_density: feat.opp_density_per_sq_mi,
                traffic_control_density: feat.traffic_control_density_per_sq_mi,
                observed_speed_mph: obs_speed as f32,
                overlap_meters: conf.total_overlap_meters as f32,
                matched_reference_count: conf.matched_reference_count as u32,
            };

            if feat.model_speed_limit_mph.is_some() {
                samples_with_sl.push(sample);
            } else {
                samples_without_sl.push(sample);
            }
        }
    }

    let out_dir = Path::new(&config.output.output_dir);
    bambam_network::fs::create_dirs(out_dir)?;

    if config.output.write_parquet {
        if !samples_with_sl.is_empty() {
            write_chunked_parquet(
                &samples_with_sl,
                &out_dir.join("calibration_with_speedlimit.parquet"),
            )?;
        }
        if !samples_without_sl.is_empty() {
            write_chunked_parquet(
                &samples_without_sl,
                &out_dir.join("calibration_without_speedlimit.parquet"),
            )?;
        }
        if !wide_rows.is_empty() {
            write_chunked_parquet(&wide_rows, &out_dir.join("conflated_edges.parquet"))?;
        }
    }

    if config.output.write_csv {
        write_records_to_csv(
            &samples_with_sl,
            &out_dir.join("calibration_with_speedlimit.csv"),
        )?;
        write_records_to_csv(
            &samples_without_sl,
            &out_dir.join("calibration_without_speedlimit.csv"),
        )?;
        write_records_to_csv(&wide_rows, &out_dir.join("conflated_edges.csv"))?;
    }

    log::info!(
        "Calibration complete: {} samples with speed limit, {} samples without speed limit written to '{}'",
        samples_with_sl.len(),
        samples_without_sl.len(),
        out_dir.display()
    );

    Ok(CalibrationSummary {
        total_open_edges: open_net.edges.len(),
        high_functional_edges: high_func_count,
        conflated_edges: conflated_count,
        samples_with_speedlimit: samples_with_sl.len(),
        samples_without_speedlimit: samples_without_sl.len(),
    })
}

/// Writes a slice of serializable records to a Snappy-compressed Parquet file in 64k batches.
pub fn write_chunked_parquet<T>(records: &[T], filepath: &Path) -> Result<(), CongestionError>
where
    T: Serialize + DeserializeOwned,
{
    if records.is_empty() {
        return Ok(());
    }
    let tracing_options = TracingOptions::default()
        .allow_null_fields(true)
        .enums_without_data_as_strings(true)
        .map_as_struct(false);
    let fields = Vec::<FieldRef>::from_type::<T>(tracing_options).map_err(|e| {
        CongestionError::Parquet(format!(
            "failed to trace Arrow schema for '{}': {e}",
            filepath.display()
        ))
    })?;

    const CHUNK_SIZE: usize = 65_536;
    let first_chunk = &records[..records.len().min(CHUNK_SIZE)];
    let first_batch = serde_arrow::to_record_batch::<&[T]>(&fields, &first_chunk).map_err(|e| {
        CongestionError::Parquet(format!(
            "failed to serialize RecordBatch for '{}': {e}",
            filepath.display()
        ))
    })?;

    let file = File::create(filepath).map_err(|source| CongestionError::Io {
        path: filepath.to_path_buf(),
        source,
    })?;
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();
    let mut writer =
        ArrowWriter::try_new(file, first_batch.schema(), Some(props)).map_err(|e| {
            CongestionError::Parquet(format!(
                "failed to initialize Parquet writer for '{}': {e}",
                filepath.display()
            ))
        })?;

    writer.write(&first_batch).map_err(|e| {
        CongestionError::Parquet(format!(
            "failed writing Parquet batch to '{}': {e}",
            filepath.display()
        ))
    })?;

    for chunk in records.chunks(CHUNK_SIZE).skip(1) {
        let batch = serde_arrow::to_record_batch::<&[T]>(&fields, &chunk).map_err(|e| {
            CongestionError::Parquet(format!(
                "failed to serialize RecordBatch for '{}': {e}",
                filepath.display()
            ))
        })?;
        writer.write(&batch).map_err(|e| {
            CongestionError::Parquet(format!(
                "failed writing Parquet batch to '{}': {e}",
                filepath.display()
            ))
        })?;
    }

    writer.close().map_err(|e| {
        CongestionError::Parquet(format!(
            "failed closing Parquet writer for '{}': {e}",
            filepath.display()
        ))
    })?;

    Ok(())
}

/// Writes a slice of serializable records to a CSV file.
pub fn write_records_to_csv<T: Serialize>(
    records: &[T],
    filepath: &Path,
) -> Result<(), CongestionError> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_path(filepath)
        .map_err(|e| CongestionError::Csv {
            path: filepath.to_path_buf(),
            message: e.to_string(),
        })?;
    for rec in records {
        writer.serialize(rec).map_err(|e| CongestionError::Csv {
            path: filepath.to_path_buf(),
            message: e.to_string(),
        })?;
    }
    writer.flush().map_err(|source| CongestionError::Io {
        path: filepath.to_path_buf(),
        source,
    })?;
    Ok(())
}
