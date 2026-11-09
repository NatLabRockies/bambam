pub mod calibrate;
pub mod config;
pub mod conflate;
pub mod error;
pub mod features;
pub mod impute;
pub mod model;
pub mod network;
pub mod taxonomy;

pub use calibrate::{run_calibration, CalibrationSampleRow, CalibrationSummary, ConflatedEdgeSummaryRow};
pub use config::{
    CalibrateCliArgs, CalibrationConfig, CalibrationOutputConfig, CensusSourceConfig,
    ConflationConfig, CongestionApp, CongestionCommand, ImputationConfig, ImputationModelConfig,
    ImputeCliArgs, OpenNetworkConfig, OpportunitiesSourceConfig, PeriodSpeedFilesConfig,
    ProjectionMode, SpatialFeaturesConfig, TomTomNetworkConfig, TrafficControlsSourceConfig,
};
pub use error::CongestionError;
pub use impute::{run_imputation, ImputationSummary, PredictedEdgeSpeedsRow};
pub use model::{SpeedPredictor, XgboostJsonModel};
pub use network::{LoadedOpenNetwork, OpenNetworkEdge};
pub use taxonomy::{CongestionRoadClass, EdgeStaticFeatures, TimePeriod};

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
        }
    }
}
