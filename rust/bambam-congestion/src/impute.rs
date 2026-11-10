use std::path::{Path, PathBuf};

use bambam_network::fs::serialize_into_enumerated_txt;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    calibrate::{write_chunked_parquet, write_records_to_csv},
    config::ImputationConfig,
    error::CongestionError,
    features::SpatialFeatureEngine,
    model::{predict_edge_for_bins, XgboostJsonModel},
    network::LoadedOpenNetwork,
};

/// Output row matching `network_with_predicted_speeds.csv` from `predict_speeds_using_models.py`,
/// plus the RouteE Compass `edge_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedEdgeSpeedsRow {
    pub edge_id: usize,
    #[serde(rename = "osm-id")]
    pub source_id: String,
    pub highway: String,
    #[serde(rename = "osm-sl-mph")]
    pub osm_sl_mph: f32,
    #[serde(rename = "wd-am-prd")]
    pub wd_am_prd: f32,
    #[serde(rename = "wd-af-prd")]
    pub wd_af_prd: f32,
    #[serde(rename = "wd-pm-prd")]
    pub wd_pm_prd: f32,
    #[serde(rename = "we-am-prd")]
    pub we_am_prd: f32,
    #[serde(rename = "we-af-prd")]
    pub we_af_prd: f32,
    #[serde(rename = "we-pm-prd")]
    pub we_pm_prd: f32,
    #[serde(rename = "sl-med-imp")]
    pub sl_med_imp: f32,
    #[serde(rename = "length-met")]
    pub length_meters: f64,
    #[serde(rename = "drive-mode")]
    pub drive_mode: bool,
}

/// Summary statistics returned by `run_imputation`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImputationSummary {
    pub total_edges: usize,
    pub high_functional_edges: usize,
    pub low_functional_edges: usize,
    pub output_dir: PathBuf,
}

/// Runs the operating speed imputation pipeline:
/// 1. Loads the Open Network (OSM/OMF) in RouteE Compass format and computes per-highway median
///    posted speed limits (`sl-med-imp`, floored at `15.0 mph`).
/// 2. Computes 0.5-mile spatial features (`Opp Density`, `Traffic Control Density`,
///    `Population Density`, `Employment Density`) for high-functional class edges (`Motorway`,
///    `Trunk`, `Link`, `Primary`, `Secondary`, `Tertiary`).
/// 3. Loads the `with_speedlimit` and `without_speedlimit` XGBoost JSON models and predicts speeds
///    across all 6 time periods (`wd-am`, `wd-af`, `wd-pm`, `we-am`, `we-af`, `we-pm`) in parallel (`rayon`).
/// 4. Writes RouteE Compass enumerated speed files (`edges-speeds-{period}-mph-enumerated.txt.gz`)
///    and summary Parquet/CSV files.
pub fn run_imputation(config: &ImputationConfig) -> Result<ImputationSummary, CongestionError> {
    let open_net = LoadedOpenNetwork::load(&config.open_network)?;

    let model_with_sl = XgboostJsonModel::from_json_file(Path::new(
        &config.models.model_with_speedlimit_path,
    ))?;
    let model_without_sl = XgboostJsonModel::from_json_file(Path::new(
        &config.models.model_without_speedlimit_path,
    ))?;

    let feature_engine = SpatialFeatureEngine::build(&open_net, &config.spatial_features)?;
    let static_features = feature_engine.compute_all_edge_features(&open_net, true);

    let predictions: Vec<Vec<f32>> = static_features
        .par_iter()
        .map(|feat| predict_edge_for_bins(feat, &config.time_bins, &model_with_sl, &model_without_sl))
        .collect();

    let out_dir = config
        .output_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| open_net.mode_dir.clone());
    bambam_network::fs::create_dirs(&out_dir)?;

    // Write the RouteE Compass enumerated speed files for all configured `time_bins`
    for (p_idx, bin) in config.time_bins.iter().enumerate() {
        let filename = bin.enumerated_speed_filename();
        let desc = format!("write {filename}");
        serialize_into_enumerated_txt(
            predictions.iter().map(|p| p[p_idx]),
            &filename,
            &out_dir,
            true,
            &desc,
        )?;
    }

    let find_bin_idx = |name: &str, fallback_idx: usize| -> Option<usize> {
        config
            .time_bins
            .iter()
            .position(|b| b.name == name)
            .or_else(|| (fallback_idx < config.time_bins.len()).then_some(fallback_idx))
    };
    let idx_wd_am = find_bin_idx("wd-am", 0);
    let idx_wd_af = find_bin_idx("wd-af", 1);
    let idx_wd_pm = find_bin_idx("wd-pm", 2);
    let idx_we_am = find_bin_idx("we-am", 3);
    let idx_we_af = find_bin_idx("we-af", 4);
    let idx_we_pm = find_bin_idx("we-pm", 5);

    let summary_rows: Vec<PredictedEdgeSpeedsRow> = open_net
        .edges
        .iter()
        .zip(static_features.iter())
        .zip(predictions.iter())
        .map(|((edge, feat), preds)| {
            let get_p = |opt_i: Option<usize>| -> f32 {
                opt_i
                    .and_then(|i| preds.get(i).copied())
                    .unwrap_or(feat.imputed_speed_limit_mph)
            };
            PredictedEdgeSpeedsRow {
                edge_id: edge.edge_id,
                source_id: edge.source_id.clone(),
                highway: edge.highway_group.clone(),
                osm_sl_mph: feat.imputed_speed_limit_mph,
                wd_am_prd: get_p(idx_wd_am),
                wd_af_prd: get_p(idx_wd_af),
                wd_pm_prd: get_p(idx_wd_pm),
                we_am_prd: get_p(idx_we_am),
                we_af_prd: get_p(idx_we_af),
                we_pm_prd: get_p(idx_we_pm),
                sl_med_imp: feat.imputed_speed_limit_mph,
                length_meters: edge.length_meters,
                drive_mode: true,
            }
        })
        .collect();

    if config.write_parquet_summary {
        write_chunked_parquet(&summary_rows, &out_dir.join("predicted-speeds.parquet"))?;
    }
    if config.write_csv_summary {
        write_records_to_csv(&summary_rows, &out_dir.join("predicted-speeds.csv"))?;
    }

    let high_func = open_net
        .edges
        .iter()
        .filter(|e| e.road_class.is_high_functional())
        .count();
    let total = open_net.edges.len();

    log::info!(
        "Imputed operating speeds for {total} edges ({high_func} high-functional via XGBoost, {} low-functional via sl-med-imp) into '{}'",
        total - high_func,
        out_dir.display()
    );

    Ok(ImputationSummary {
        total_edges: total,
        high_functional_edges: high_func,
        low_functional_edges: total - high_func,
        output_dir: out_dir,
    })
}
