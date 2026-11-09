pub mod xgboost_json;

pub use xgboost_json::XgboostJsonModel;

use crate::taxonomy::{EdgeStaticFeatures, TimePeriod};

/// Trait implemented by operating-speed prediction models (e.g., `XgboostJsonModel` or an ONNX model).
pub trait SpeedPredictor: Send + Sync {
    /// Evaluates a single feature slice and returns the predicted operating speed in mph.
    fn predict(&self, features: &[f32]) -> f32;

    /// Expected number of input features (`16` for `with_speedlimit`, `15` for `without_speedlimit`).
    fn num_features(&self) -> usize;
}

/// Evaluates all 6 `TimePeriod`s (`[wd-am, wd-af, wd-pm, we-am, we-af, we-pm]`) for a single edge.
///
/// - Low-functional class roads (`Residential`, `Service`, `Other`) return `imputed_speed_limit_mph` (`sl-med-imp`)
///   for all 6 periods, matching `predict_speeds_using_models.py`.
/// - High-functional class roads with `Some(sl)` (`Road Class != Link`) are evaluated with `model_with_sl`.
/// - High-functional class roads with `None` (`Road Class == Link` or missing posted speed limit)
///   are evaluated with `model_without_sl`.
pub fn predict_edge_all_periods<M1, M2>(
    static_features: &EdgeStaticFeatures,
    model_with_sl: &M1,
    model_without_sl: &M2,
) -> [f32; 6]
where
    M1: SpeedPredictor,
    M2: SpeedPredictor,
{
    if !static_features.road_class.is_high_functional() {
        return [static_features.imputed_speed_limit_mph; 6];
    }

    let mut out = [0.0_f32; 6];
    match static_features.model_speed_limit_mph {
        Some(sl) => {
            for (idx, period) in TimePeriod::ALL.into_iter().enumerate() {
                let row = static_features.to_features_with_speedlimit(period, sl);
                out[idx] = model_with_sl.predict(&row);
            }
        }
        None => {
            for (idx, period) in TimePeriod::ALL.into_iter().enumerate() {
                let row = static_features.to_features_without_speedlimit(period);
                out[idx] = model_without_sl.predict(&row);
            }
        }
    }
    out
}
