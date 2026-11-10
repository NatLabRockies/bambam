pub mod xgboost_json;

pub use xgboost_json::XgboostJsonModel;

use crate::taxonomy::{EdgeStaticFeatures, TimeBinSpec, TimePeriod};

/// Trait implemented by operating-speed prediction models (e.g., `XgboostJsonModel` or an ONNX model).
pub trait SpeedPredictor: Send + Sync {
    /// Evaluates a single feature slice and returns the predicted operating speed in mph.
    fn predict(&self, features: &[f32]) -> f32;

    /// Expected number of input features (`16` for `with_speedlimit`, `15` for `without_speedlimit`).
    fn num_features(&self) -> usize;
}

/// Evaluates all configured `TimeBinSpec`s for a single edge.
///
/// - Low-functional class roads (`Residential`, `Service`, `Other`) return `imputed_speed_limit_mph` (`sl-med-imp`)
///   for all time bins, matching `predict_speeds_using_models.py`.
/// - High-functional class roads with `Some(sl)` (`Road Class != Link`) are evaluated with `model_with_sl`.
/// - High-functional class roads with `None` (`Road Class == Link` or missing posted speed limit)
///   are evaluated with `model_without_sl`.
pub fn predict_edge_for_bins<M1, M2>(
    static_features: &EdgeStaticFeatures,
    time_bins: &[TimeBinSpec],
    model_with_sl: &M1,
    model_without_sl: &M2,
) -> Vec<f32>
where
    M1: SpeedPredictor,
    M2: SpeedPredictor,
{
    if !static_features.road_class.is_high_functional() {
        return vec![static_features.imputed_speed_limit_mph; time_bins.len()];
    }

    match static_features.model_speed_limit_mph {
        Some(sl) => time_bins
            .iter()
            .map(|bin| {
                let row = static_features.to_features_with_speedlimit_for_bin(bin, sl);
                model_with_sl.predict(&row)
            })
            .collect(),
        None => time_bins
            .iter()
            .map(|bin| {
                let row = static_features.to_features_without_speedlimit_for_bin(bin);
                model_without_sl.predict(&row)
            })
            .collect(),
    }
}

/// Evaluates all 6 default `TimePeriod`s (`[wd-am, wd-af, wd-pm, we-am, we-af, we-pm]`) for a single edge.
pub fn predict_edge_all_periods<M1, M2>(
    static_features: &EdgeStaticFeatures,
    model_with_sl: &M1,
    model_without_sl: &M2,
) -> [f32; 6]
where
    M1: SpeedPredictor,
    M2: SpeedPredictor,
{
    let bins = TimeBinSpec::default_six_bins();
    let vec_preds = predict_edge_for_bins(static_features, &bins, model_with_sl, model_without_sl);
    let mut out = [0.0_f32; 6];
    for (i, v) in vec_preds.into_iter().enumerate().take(6) {
        out[i] = v;
    }
    let _ = TimePeriod::ALL;
    out
}
