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
    BamcensusLodesConfig, BamcensusPopulationConfig, CalibrateCliArgs, CalibrationConfig,
    CalibrationOutputConfig, CensusSourceConfig, CensusStudyRegion, ConflationConfig,
    CongestionApp, CongestionCommand, CongestionPipelineConfig, ExportCensusCliArgs,
    ImputationConfig, ImputationModelConfig, ImputeCliArgs, ListTimeBinsCliArgs, OpenNetworkConfig,
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
            CongestionCommand::ExportCensus(args) => {
                let mut pipeline_cfg = if let Some(ref p) = args.config {
                    CongestionPipelineConfig::from_file(std::path::Path::new(p))?
                } else {
                    CongestionPipelineConfig {
                        open_network: OpenNetworkConfig::default(),
                        time_bins: TimeBinSpec::default_six_bins(),
                        spatial_features: SpatialFeaturesConfig::default(),
                        reference_network: None,
                        conflation: ConflationConfig::default(),
                        artifacts: PipelineArtifactsConfig::default(),
                    }
                };
                if let Some(ref open_dir) = args.open_network_dir {
                    pipeline_cfg.open_network.network_dir = open_dir.clone();
                }
                let open_net = LoadedOpenNetwork::load(&pipeline_cfg.open_network)?;
                let (pop_src, emp_src) = match pipeline_cfg.spatial_features.census_source {
                    Some(CensusSourceConfig::Bamcensus {
                        population_source,
                        employment_source,
                        ..
                    }) => (population_source, employment_source),
                    _ => (
                        BamcensusPopulationConfig::default(),
                        BamcensusLodesConfig::default(),
                    ),
                };
                let census_cfg = CensusSourceConfig::Bamcensus {
                    population_source: pop_src,
                    employment_source: emp_src,
                    save_csv_path: Some(args.output_csv.clone()),
                };
                crate::features::census::build_census_rtree(
                    Some(&census_cfg),
                    open_net.bbox,
                    pipeline_cfg.spatial_features.density_radius_miles,
                    pipeline_cfg.spatial_features.projection_mode,
                )?;
                Ok(())
            }
        }
    }
}
