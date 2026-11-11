use std::path::Path;

use clap::{Args, Parser, Subcommand};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    calibrate::run_calibration, error::CongestionError, impute::run_imputation,
    network::LoadedOpenNetwork, taxonomy::TimeBinSpec,
};

use super::{
    BamcensusLodesConfig, BamcensusPopulationConfig, CalibrationConfig, CalibrationOutputConfig,
    CensusSourceConfig, ConflationConfig, CongestionPipelineConfig, ImputationConfig,
    ImputationModelConfig, OpenNetworkConfig, OpportunitiesSourceConfig, PipelineArtifactsConfig,
    ReferenceNetworkConfig, ReferenceSpeedFilesConfig, SpatialFeaturesConfig,
};

/// CLI entry point for `bambam-congestion`.
#[derive(Debug, Parser)]
#[command(
    name = "bambam-congestion",
    author,
    version,
    about = "Congestion calibration and operating speed imputation for BAMBAM and RouteE Compass road networks"
)]
#[command(propagate_version = true)]
pub struct CongestionApp {
    #[command(subcommand)]
    pub command: CongestionCommand,
}

#[derive(Debug, Clone, Subcommand, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CongestionCommand {
    /// Conflate an Open Network (OSM/OMF) with a Reference Road Network in RouteE Compass format,
    /// compute 0.5-mile spatial features, and export calibration training tables (Parquet/CSV).
    Calibrate(CalibrateCliArgs),

    /// Impute operating speeds across all configured `time_bins` for a BAMBAM / RouteE Compass
    /// road network using calibrated XGBoost models.
    Impute(ImputeCliArgs),

    /// Print the configured time-bin names and indicator flags (from `--config` or the default 6 BAMBAM time bins).
    ListTimeBins(ListTimeBinsCliArgs),

    /// Download Census TIGER/Line polygons tagged with ACS population and LEHD LODES employment
    /// via `bamcensus` for an Open Network's bounding box and save them to a CSV (or `.csv.gz`) file.
    ExportCensus(ExportCensusCliArgs),
}

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
                    let cfg = CongestionPipelineConfig::from_file(Path::new(p))?;
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
                    CongestionPipelineConfig::from_file(Path::new(p))?
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

/// CLI arguments for `bambam-congestion export-census`.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct ExportCensusCliArgs {
    /// Optional path to a `congestion.toml` configuration file.
    #[arg(short, long)]
    pub config: Option<String>,

    /// Optional override: path to the Open Network directory (used to determine the study bounding box).
    #[arg(long)]
    pub open_network_dir: Option<String>,

    /// Output CSV (or `.csv.gz`) file path to write the `bamcensus` polygons (`geoid,population,jobs,geometry`).
    #[arg(short, long)]
    pub output_csv: String,
}

/// CLI arguments for `bambam-congestion list-time-bins`.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct ListTimeBinsCliArgs {
    /// Optional path to a `congestion.toml` configuration file. If omitted, prints the default 6 BAMBAM time bins.
    #[arg(short, long)]
    pub config: Option<String>,
}

/// CLI arguments for `bambam-congestion calibrate`.
///
/// Accepts a unified `congestion.toml` (`CongestionPipelineConfig`) or standalone `CalibrationConfig`
/// via `--config <path.toml|.json>`, with optional CLI flag overrides.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct CalibrateCliArgs {
    /// Path to a TOML or JSON configuration file (`congestion.toml`).
    #[arg(short, long)]
    pub config: Option<String>,

    /// Override: path to the Open Network (OSM/OMF) RouteE Compass directory.
    #[arg(long)]
    pub open_network_dir: Option<String>,

    /// Override: travel mode subdirectory in the Open Network (defaults to `"drive"`).
    #[arg(long)]
    pub mode: Option<String>,

    /// Override: path to the Reference Road Network RouteE Compass directory.
    #[arg(long, visible_alias = "tomtom-network-dir")]
    pub reference_network_dir: Option<String>,

    /// Override: directory containing enumerated reference speed files per time bin.
    #[arg(long, visible_alias = "tomtom-speeds-dir")]
    pub reference_speeds_dir: Option<String>,

    /// Override: filename prefix for reference speed files in `--reference-speeds-dir`
    /// (`<prefix>-<bin>.txt.gz`).
    #[arg(long)]
    pub reference_file_prefix: Option<String>,

    /// Override: output directory for calibration Parquet/CSV tables.
    #[arg(short, long)]
    pub output_dir: Option<String>,

    /// Optional `.osm.pbf` path for extracting MEP land-use opportunities and traffic controls.
    #[arg(long)]
    pub osm_pbf_path: Option<String>,

    /// Optional local Overture Maps Parquet directory (`raw/` or directory containing `places`/`buildings` Parquet files)
    /// when `opportunities_source.type = "overture_maps"`.
    #[arg(long)]
    pub omf_local_source_dir: Option<String>,

    /// Optional CSV (or `.csv.gz`) path of Census polygons (`geoid,population,jobs,geometry`,
    /// such as the file produced by `bambam-congestion export-census`) for computing population and employment densities.
    #[arg(long)]
    pub census_csv: Option<String>,

    /// Optional CSV (or `.csv.gz`) path to save the downloaded `bamcensus` dataset
    /// (`geoid,population,jobs,geometry`) when using `CensusSourceConfig::Bamcensus`.
    #[arg(long)]
    pub save_bamcensus_csv: Option<String>,
}

impl CalibrateCliArgs {
    pub fn resolve_config(&self) -> Result<CalibrationConfig, CongestionError> {
        let mut cfg = if let Some(ref config_path) = self.config {
            let path = Path::new(config_path);
            if let Ok(pipeline) = CongestionPipelineConfig::from_file(path) {
                pipeline.into_calibration_config()?
            } else {
                CalibrationConfig::from_file(path)?
            }
        } else {
            let open_dir = self.open_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --open-network-dir".to_string(),
                )
            })?;
            let ref_dir = self.reference_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --reference-network-dir".to_string(),
                )
            })?;
            let speeds_dir = self.reference_speeds_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --reference-speeds-dir".to_string(),
                )
            })?;
            let output_dir = self.output_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --output-dir".to_string(),
                )
            })?;
            CalibrationConfig {
                open_network: OpenNetworkConfig {
                    network_dir: open_dir,
                    mode: self.mode.clone().unwrap_or_else(|| "drive".to_string()),
                    ..Default::default()
                },
                time_bins: TimeBinSpec::default_six_bins(),
                reference_network: ReferenceNetworkConfig {
                    network_dir: ref_dir,
                    speed_files: ReferenceSpeedFilesConfig {
                        speeds_dir: Some(speeds_dir),
                        file_prefix: self.reference_file_prefix.clone(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                conflation: ConflationConfig::default(),
                spatial_features: SpatialFeaturesConfig::default(),
                output: CalibrationOutputConfig {
                    output_dir,
                    ..Default::default()
                },
            }
        };

        if let Some(ref open_dir) = self.open_network_dir {
            cfg.open_network.network_dir = open_dir.clone();
        }
        if let Some(ref mode) = self.mode {
            cfg.open_network.mode = mode.clone();
        }
        if let Some(ref ref_dir) = self.reference_network_dir {
            cfg.reference_network.network_dir = ref_dir.clone();
        }
        if let Some(ref speeds_dir) = self.reference_speeds_dir {
            cfg.reference_network.speed_files.speeds_dir = Some(speeds_dir.clone());
        }
        if let Some(ref prefix) = self.reference_file_prefix {
            cfg.reference_network.speed_files.file_prefix = Some(prefix.clone());
        }
        if let Some(ref out_dir) = self.output_dir {
            cfg.output.output_dir = out_dir.clone();
        }
        if let Some(ref pbf_path) = self.osm_pbf_path {
            cfg.spatial_features.opportunities_source = Some(OpportunitiesSourceConfig::OsmPbf {
                pbf_path: pbf_path.clone(),
            });
        }
        if let Some(ref omf_dir) = self.omf_local_source_dir {
            match cfg.spatial_features.opportunities_source {
                Some(OpportunitiesSourceConfig::OvertureMaps {
                    ref mut local_source_dir,
                    ..
                }) => {
                    *local_source_dir = Some(omf_dir.clone());
                }
                _ => {
                    cfg.spatial_features.opportunities_source =
                        Some(OpportunitiesSourceConfig::OvertureMaps {
                            local_source_dir: Some(omf_dir.clone()),
                            include_buildings: true,
                        });
                }
            }
        }
        if let Some(ref census_csv_path) = self.census_csv {
            cfg.spatial_features.census_source = Some(CensusSourceConfig::CsvPolygons {
                polygons_csv_path: census_csv_path.clone(),
                geoid_column: "geoid".to_string(),
                wkt_column: "geometry".to_string(),
                population_column: Some("population".to_string()),
                employment_column: Some("jobs".to_string()),
                population_csv_path: None,
                population_csv_geoid_column: None,
                population_csv_value_column: None,
                employment_csv_path: None,
                employment_csv_geoid_column: None,
                employment_csv_value_column: None,
            });
        }
        if let Some(ref save_path) = self.save_bamcensus_csv {
            match cfg.spatial_features.census_source {
                Some(CensusSourceConfig::Bamcensus {
                    ref mut save_csv_path,
                    ..
                }) => {
                    *save_csv_path = Some(save_path.clone());
                }
                _ => {
                    cfg.spatial_features.census_source = Some(CensusSourceConfig::Bamcensus {
                        population_source: BamcensusPopulationConfig::default(),
                        employment_source: BamcensusLodesConfig::default(),
                        save_csv_path: Some(save_path.clone()),
                    });
                }
            }
        }

        if cfg.open_network.network_dir.trim().is_empty() {
            return Err(CongestionError::Configuration(
                "missing open network directory: provide --open-network-dir or set [open_network].network_dir in config".to_string(),
            ));
        }
        if cfg.reference_network.network_dir.trim().is_empty() {
            return Err(CongestionError::Configuration(
                "missing reference network directory: provide --reference-network-dir or set [reference_network].network_dir in config".to_string(),
            ));
        }
        if cfg.output.output_dir.trim().is_empty() {
            return Err(CongestionError::Configuration(
                "missing calibration output directory: provide --output-dir or set [artifacts].calibration_dir in config".to_string(),
            ));
        }

        Ok(cfg)
    }
}

/// CLI arguments for `bambam-congestion impute`.
///
/// Accepts the same unified `congestion.toml` (`CongestionPipelineConfig`) used for `calibrate`
/// or a standalone `ImputationConfig` via `--config <path.toml|.json>`.
#[derive(Debug, Clone, Args, Serialize, Deserialize, JsonSchema)]
pub struct ImputeCliArgs {
    /// Path to a TOML or JSON configuration file (`congestion.toml`).
    #[arg(short, long)]
    pub config: Option<String>,

    /// Override: path to the Open Network (OSM/OMF) RouteE Compass directory.
    #[arg(long)]
    pub open_network_dir: Option<String>,

    /// Override: travel mode subdirectory in the Open Network (defaults to `"drive"`).
    #[arg(long)]
    pub mode: Option<String>,

    /// Override: directory containing `xgboost_model_with_speedlimit.json` and `xgboost_model_without_speedlimit.json`.
    #[arg(long)]
    pub models_dir: Option<String>,

    /// Override: path to the calibrated `with_speedlimit` XGBoost `.json` model file.
    #[arg(long)]
    pub model_with_speedlimit: Option<String>,

    /// Override: path to the calibrated `without_speedlimit` XGBoost `.json` model file.
    #[arg(long)]
    pub model_without_speedlimit: Option<String>,

    /// Override: optional output directory (defaults to `<open_network_dir>/<mode>`).
    #[arg(short, long)]
    pub output_dir: Option<String>,

    /// Optional `.osm.pbf` path for extracting MEP land-use opportunities and traffic controls.
    #[arg(long)]
    pub osm_pbf_path: Option<String>,

    /// Optional local Overture Maps Parquet directory (`raw/` or directory containing `places`/`buildings` Parquet files)
    /// when `opportunities_source.type = "overture_maps"`.
    #[arg(long)]
    pub omf_local_source_dir: Option<String>,

    /// Optional CSV (or `.csv.gz`) path of Census polygons (`geoid,population,jobs,geometry`,
    /// such as the file produced by `bambam-congestion export-census`) for computing population and employment densities.
    #[arg(long)]
    pub census_csv: Option<String>,

    /// Optional CSV (or `.csv.gz`) path to save the downloaded `bamcensus` dataset
    /// (`geoid,population,jobs,geometry`) when using `CensusSourceConfig::Bamcensus`.
    #[arg(long)]
    pub save_bamcensus_csv: Option<String>,
}

impl ImputeCliArgs {
    pub fn resolve_config(&self) -> Result<ImputationConfig, CongestionError> {
        let mut cfg = if let Some(ref config_path) = self.config {
            let path = Path::new(config_path);
            if let Ok(pipeline) = CongestionPipelineConfig::from_file(path) {
                pipeline.into_imputation_config()
            } else {
                ImputationConfig::from_file(path)?
            }
        } else {
            let open_dir = self.open_network_dir.clone().ok_or_else(|| {
                CongestionError::Configuration(
                    "must provide either --config or --open-network-dir".to_string(),
                )
            })?;
            let default_with_sl = self.models_dir.as_ref().map(|d| {
                Path::new(d)
                    .join("xgboost_model_with_speedlimit.json")
                    .to_string_lossy()
                    .into_owned()
            });
            let default_without_sl = self.models_dir.as_ref().map(|d| {
                Path::new(d)
                    .join("xgboost_model_without_speedlimit.json")
                    .to_string_lossy()
                    .into_owned()
            });
            let model_with_sl = self
                .model_with_speedlimit
                .clone()
                .or(default_with_sl)
                .ok_or_else(|| {
                    CongestionError::Configuration(
                        "must provide --config, --models-dir, or --model-with-speedlimit"
                            .to_string(),
                    )
                })?;
            let model_without_sl = self
                .model_without_speedlimit
                .clone()
                .or(default_without_sl)
                .ok_or_else(|| {
                    CongestionError::Configuration(
                        "must provide --config, --models-dir, or --model-without-speedlimit"
                            .to_string(),
                    )
                })?;
            ImputationConfig {
                open_network: OpenNetworkConfig {
                    network_dir: open_dir,
                    mode: self.mode.clone().unwrap_or_else(|| "drive".to_string()),
                    ..Default::default()
                },
                time_bins: TimeBinSpec::default_six_bins(),
                spatial_features: SpatialFeaturesConfig::default(),
                models: ImputationModelConfig {
                    model_with_speedlimit_path: model_with_sl,
                    model_without_speedlimit_path: model_without_sl,
                },
                output_dir: self.output_dir.clone(),
                write_parquet_summary: true,
                write_csv_summary: true,
            }
        };

        if let Some(ref open_dir) = self.open_network_dir {
            cfg.open_network.network_dir = open_dir.clone();
        }
        if let Some(ref mode) = self.mode {
            cfg.open_network.mode = mode.clone();
        }
        if let Some(ref models_dir) = self.models_dir {
            let base = Path::new(models_dir);
            cfg.models.model_with_speedlimit_path = base
                .join("xgboost_model_with_speedlimit.json")
                .to_string_lossy()
                .into_owned();
            cfg.models.model_without_speedlimit_path = base
                .join("xgboost_model_without_speedlimit.json")
                .to_string_lossy()
                .into_owned();
        }
        if let Some(ref m_with) = self.model_with_speedlimit {
            cfg.models.model_with_speedlimit_path = m_with.clone();
        }
        if let Some(ref m_without) = self.model_without_speedlimit {
            cfg.models.model_without_speedlimit_path = m_without.clone();
        }
        if let Some(ref out_dir) = self.output_dir {
            cfg.output_dir = Some(out_dir.clone());
        }
        if let Some(ref pbf_path) = self.osm_pbf_path {
            cfg.spatial_features.opportunities_source = Some(OpportunitiesSourceConfig::OsmPbf {
                pbf_path: pbf_path.clone(),
            });
        }
        if let Some(ref omf_dir) = self.omf_local_source_dir {
            match cfg.spatial_features.opportunities_source {
                Some(OpportunitiesSourceConfig::OvertureMaps {
                    ref mut local_source_dir,
                    ..
                }) => {
                    *local_source_dir = Some(omf_dir.clone());
                }
                _ => {
                    cfg.spatial_features.opportunities_source =
                        Some(OpportunitiesSourceConfig::OvertureMaps {
                            local_source_dir: Some(omf_dir.clone()),
                            include_buildings: true,
                        });
                }
            }
        }
        if let Some(ref census_csv_path) = self.census_csv {
            cfg.spatial_features.census_source = Some(CensusSourceConfig::CsvPolygons {
                polygons_csv_path: census_csv_path.clone(),
                geoid_column: "geoid".to_string(),
                wkt_column: "geometry".to_string(),
                population_column: Some("population".to_string()),
                employment_column: Some("jobs".to_string()),
                population_csv_path: None,
                population_csv_geoid_column: None,
                population_csv_value_column: None,
                employment_csv_path: None,
                employment_csv_geoid_column: None,
                employment_csv_value_column: None,
            });
        }
        if let Some(ref save_path) = self.save_bamcensus_csv {
            match cfg.spatial_features.census_source {
                Some(CensusSourceConfig::Bamcensus {
                    ref mut save_csv_path,
                    ..
                }) => {
                    *save_csv_path = Some(save_path.clone());
                }
                _ => {
                    cfg.spatial_features.census_source = Some(CensusSourceConfig::Bamcensus {
                        population_source: BamcensusPopulationConfig::default(),
                        employment_source: BamcensusLodesConfig::default(),
                        save_csv_path: Some(save_path.clone()),
                    });
                }
            }
        }

        if cfg.open_network.network_dir.trim().is_empty() {
            return Err(CongestionError::Configuration(
                "missing open network directory: provide --open-network-dir or set [open_network].network_dir in config".to_string(),
            ));
        }

        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_congestion_toml() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("configuration/bambam-congestion/congestion.toml")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn test_calibrate_cli_resolves_pathless_congestion_toml() {
        let toml_path = repo_congestion_toml();

        // Missing required paths when only pathless congestion.toml is given should fail cleanly
        let missing_args = CalibrateCliArgs {
            config: Some(toml_path.clone()),
            open_network_dir: None,
            mode: None,
            reference_network_dir: None,
            reference_speeds_dir: None,
            reference_file_prefix: None,
            output_dir: None,
            osm_pbf_path: None,
            omf_local_source_dir: None,
            census_csv: None,
            save_bamcensus_csv: None,
        };
        assert!(missing_args.resolve_config().is_err());

        // Passing paths via CLI overrides populates CalibrationConfig cleanly
        let args = CalibrateCliArgs {
            config: Some(toml_path),
            open_network_dir: Some("us_network".to_string()),
            mode: Some("drive".to_string()),
            reference_network_dir: Some("/path/to/tomtom".to_string()),
            reference_speeds_dir: Some("/path/to/speeds".to_string()),
            reference_file_prefix: None,
            output_dir: Some("output/calib_us".to_string()),
            osm_pbf_path: None,
            omf_local_source_dir: None,
            census_csv: Some("us_network/census_tracts_bamcensus.csv.gz".to_string()),
            save_bamcensus_csv: None,
        };
        let cfg = args.resolve_config().expect("should resolve calibrate config");
        assert_eq!(cfg.open_network.network_dir, "us_network");
        assert_eq!(cfg.open_network.mode, "drive");
        assert_eq!(cfg.reference_network.network_dir, "/path/to/tomtom");
        assert_eq!(
            cfg.reference_network.speed_files.speeds_dir.as_deref(),
            Some("/path/to/speeds")
        );
        assert_eq!(
            cfg.reference_network.speed_files.file_prefix.as_deref(),
            Some("tomtom-natl")
        );
        assert_eq!(cfg.output.output_dir, "output/calib_us");
        match cfg.spatial_features.census_source {
            Some(CensusSourceConfig::CsvPolygons {
                ref polygons_csv_path,
                ref geoid_column,
                ref wkt_column,
                ref population_column,
                ref employment_column,
                ..
            }) => {
                assert_eq!(
                    polygons_csv_path,
                    "us_network/census_tracts_bamcensus.csv.gz"
                );
                assert_eq!(geoid_column, "geoid");
                assert_eq!(wkt_column, "geometry");
                assert_eq!(population_column.as_deref(), Some("population"));
                assert_eq!(employment_column.as_deref(), Some("jobs"));
            }
            other => panic!("expected CsvPolygons census_source, got {other:?}"),
        }
    }

    #[test]
    fn test_impute_cli_resolves_pathless_congestion_toml_and_models_dir() {
        let toml_path = repo_congestion_toml();

        let args = ImputeCliArgs {
            config: Some(toml_path),
            open_network_dir: Some("us_network".to_string()),
            mode: Some("drive".to_string()),
            models_dir: Some("my_models_dir".to_string()),
            model_with_speedlimit: None,
            model_without_speedlimit: None,
            output_dir: Some("us_network/drive".to_string()),
            osm_pbf_path: None,
            omf_local_source_dir: None,
            census_csv: Some("us_network/census_tracts_bamcensus.csv.gz".to_string()),
            save_bamcensus_csv: None,
        };
        let cfg = args.resolve_config().expect("should resolve impute config");
        assert_eq!(cfg.open_network.network_dir, "us_network");
        assert_eq!(cfg.output_dir.as_deref(), Some("us_network/drive"));
        assert_eq!(
            cfg.models.model_with_speedlimit_path,
            "my_models_dir/xgboost_model_with_speedlimit.json"
        );
        assert_eq!(
            cfg.models.model_without_speedlimit_path,
            "my_models_dir/xgboost_model_without_speedlimit.json"
        );
    }
}
