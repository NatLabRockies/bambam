pub mod calibrate;
pub mod config;
pub mod conflate;
pub mod error;
pub mod features;
pub mod impute;
pub mod model;
pub mod network;
pub mod taxonomy;

pub use calibrate::{
    run_calibration, CalibrationSampleRow, CalibrationSummary, ConflatedEdgeSummaryRow,
};
pub use config::{
    CalibrateCliArgs, CalibrationConfig, CalibrationOutputConfig, CensusSourceConfig,
    ConflationConfig, CongestionApp, CongestionCommand, CongestionPipelineConfig, ImputationConfig,
    ImputationModelConfig, ImputeCliArgs, ListTimeBinsCliArgs, OpenNetworkConfig,
    OpportunitiesSourceConfig, PipelineArtifactsConfig, ProjectionMode, ReferenceNetworkConfig,
    ReferenceSpeedFilesConfig, SpatialFeaturesConfig, TrafficControlsSourceConfig,
};
pub use error::CongestionError;
pub use impute::{run_imputation, ImputationSummary, PredictedEdgeSpeedsRow};
pub use model::{SpeedPredictor, XgboostJsonModel};
pub use network::{LoadedOpenNetwork, OpenNetworkEdge};
pub use taxonomy::{CongestionRoadClass, EdgeStaticFeatures, TimeBinSpec, TimePeriod};

impl CongestionCommand {
    pub fn run(&self) -> Result<(), CongestionError> {
        match self {
            CongestionCommand::Calibrate(args) => {
                let cfg = args.resolve_config()?;
                run_calibration(&cfg).map(|_| ())
            }
            CongestionCommand::Impute(args) => {
                let cfg = args.resolve_config()?;
                run_imputation(&cfg).map(|_| ())
            }
            CongestionCommand::ListTimeBins(args) => {
                let bins = if let Some(ref p) = args.config {
                    let cfg = CongestionPipelineConfig::from_file(std::path::Path::new(p))?;
                    cfg.time_bins
                } else {
                    TimeBinSpec::default_six_bins()
                };
                for b in &bins {
                    let (day_wd, hr_am, hr_pm) = b.time_flags();
                    println!(
                        "{}\tweekday={}\tstart={}\tend={}\t(day_weekday={day_wd}, hour_am_peak={hr_am}, hour_pm_peak={hr_pm})",
                        b.name, b.weekday, b.start, b.end
                    );
                }
                Ok(())
            }
        }
    }
}
