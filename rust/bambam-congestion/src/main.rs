use bambam_congestion::{CongestionApp, CongestionError};
use clap::Parser;

fn main() -> Result<(), CongestionError> {
    let _ = env_logger::try_init();
    let app = CongestionApp::parse();
    app.command.run()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use bambam_congestion::{
        run_calibration, run_imputation, ConflationConfig, CongestionPipelineConfig,
        OpenNetworkConfig, PipelineArtifactsConfig, ReferenceNetworkConfig,
        ReferenceSpeedFilesConfig, SpatialFeaturesConfig, TimeBinSpec,
    };
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;

    fn write_gz_lines(path: &Path, lines: &[&str]) {
        let f = fs::File::create(path).unwrap();
        let mut gz = GzEncoder::new(f, Compression::default());
        for line in lines {
            writeln!(gz, "{line}").unwrap();
        }
        gz.finish().unwrap();
    }

    #[test]
    fn test_end_to_end_unified_congestion_pipeline_config() {
        let tmp_root = Path::new("target/test_tmp_congestion");
        let _ = fs::remove_dir_all(tmp_root);
        let open_drive_dir = tmp_root.join("open_net").join("drive");
        let ref_dir = tmp_root.join("reference_net");
        let calib_out_dir = tmp_root.join("calib_out");
        let models_dir = tmp_root.join("models");
        fs::create_dir_all(&open_drive_dir).unwrap();
        fs::create_dir_all(&ref_dir).unwrap();
        fs::create_dir_all(&calib_out_dir).unwrap();
        fs::create_dir_all(&models_dir).unwrap();

        // 1. Create a 3-edge synthetic Open Network inside Boulder (-105.260, 40.015):
        //    - edge 0: primary road with posted speed 35 mph (Eastbound)
        //    - edge 1: motorway_link (Link class -> forces without_speedlimit model) with posted speed 45 mph (Eastbound)
        //    - edge 2: residential road with missing posted speed (low-functional -> uses sl-med-imp floor = 15 mph)
        write_gz_lines(
            &open_drive_dir.parent().unwrap().join("vertices-complete.csv.gz"),
            &[
                "vertex_id,connector_id,x,y,traffic_signals,stop,give_way,crossing,infrastructure_ids",
                "0,n0,-105.2600,40.0150,true,false,false,true,sig1",
                "1,n1,-105.2580,40.0150,false,true,false,false,stp1",
                "2,n2,-105.2560,40.0150,false,false,false,false,",
                "3,n3,-105.2540,40.0150,false,false,false,false,",
            ],
        );
        write_gz_lines(
            &open_drive_dir.join("edges-compass.csv.gz"),
            &[
                "edge_id,src_vertex_id,dst_vertex_id,distance",
                "0,0,1,170.0",
                "1,1,2,170.0",
                "2,2,3,170.0",
            ],
        );
        write_gz_lines(
            &open_drive_dir.join("edges-geometries-enumerated.txt.gz"),
            &[
                "LINESTRING(-105.2600 40.0150,-105.2580 40.0150)",
                "LINESTRING(-105.2580 40.0150,-105.2560 40.0150)",
                "LINESTRING(-105.2560 40.0150,-105.2540 40.0150)",
            ],
        );
        write_gz_lines(
            &open_drive_dir.join("edges-classes-enumerated.txt.gz"),
            &["road-primary", "road-motorway-link", "road-residential"],
        );
        write_gz_lines(
            &open_drive_dir.join("edges-posted-speeds-mph-enumerated.txt.gz"),
            &["35.0", "45.0", ""],
        );
        write_gz_lines(
            &open_drive_dir.join("edges-speeds-mph-enumerated.txt.gz"),
            &["35.0", "45.0", "25.0"],
        );

        // 2. Create synthetic Reference network matching edges 0 and 1 (within 2m north, same Eastbound heading)
        write_gz_lines(
            &ref_dir.join("edges-geometries-enumerated.txt.gz"),
            &[
                "LINESTRING(-105.2600 40.01501,-105.2580 40.01501)",
                "LINESTRING(-105.2580 40.01501,-105.2560 40.01501)",
            ],
        );
        let period_speeds_kph = [
            ("wd-am", ["48.0", "64.0"]),
            ("wd-af", ["52.0", "70.0"]),
            ("wd-pm", ["44.0", "60.0"]),
            ("we-am", ["56.0", "75.0"]),
            ("we-af", ["54.0", "72.0"]),
            ("we-pm", ["50.0", "68.0"]),
        ];
        for (p_str, vals) in &period_speeds_kph {
            write_gz_lines(&ref_dir.join(format!("ref-speeds-{p_str}.txt.gz")), vals);
        }

        // 3. Build a single unified CongestionPipelineConfig (`congestion.toml`) and verify TOML roundtrip
        let fixture_census_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("boulder_co")
            .join("census_tracts_bamcensus.csv.gz");
        let spatial_features = if fixture_census_path.exists() {
            SpatialFeaturesConfig {
                census_source: Some(bambam_congestion::CensusSourceConfig::CsvPolygons {
                    polygons_csv_path: fixture_census_path.to_string_lossy().into_owned(),
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
                }),
                ..Default::default()
            }
        } else {
            SpatialFeaturesConfig::default()
        };

        let pipeline_cfg = CongestionPipelineConfig {
            open_network: OpenNetworkConfig {
                network_dir: open_drive_dir
                    .parent()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                mode: "drive".to_string(),
                ..Default::default()
            },
            time_bins: TimeBinSpec::default_six_bins(),
            spatial_features,
            reference_network: Some(ReferenceNetworkConfig {
                network_dir: ref_dir.to_string_lossy().into_owned(),
                require_time_varying_speeds: true,
                speed_files: ReferenceSpeedFilesConfig::from_directory(&ref_dir, "ref-speeds"),
                ..Default::default()
            }),
            conflation: ConflationConfig::default(),
            artifacts: PipelineArtifactsConfig {
                calibration_dir: calib_out_dir.to_string_lossy().into_owned(),
                models_dir: models_dir.to_string_lossy().into_owned(),
                ..Default::default()
            },
        };

        assert_eq!(
            pipeline_cfg.time_bin_names(),
            vec!["wd-am", "wd-af", "wd-pm", "we-am", "we-af", "we-pm"]
        );

        let toml_path = tmp_root.join("congestion.toml");
        fs::write(&toml_path, toml::to_string_pretty(&pipeline_cfg).unwrap()).unwrap();
        let loaded_pipeline = CongestionPipelineConfig::from_file(&toml_path).unwrap();

        // 4. Run Calibration from the unified config
        let calib_cfg = loaded_pipeline.clone().into_calibration_config().unwrap();
        let calib_summary = run_calibration(&calib_cfg).expect("calibration failed");
        assert_eq!(calib_summary.total_open_edges, 3);
        assert_eq!(calib_summary.high_functional_edges, 2);
        assert_eq!(calib_summary.conflated_edges, 2);
        assert_eq!(calib_summary.samples_with_speedlimit, 6);
        assert_eq!(calib_summary.samples_without_speedlimit, 6);
        assert!(calib_out_dir
            .join("calibration_with_speedlimit.parquet")
            .exists());
        assert!(calib_out_dir
            .join("calibration_without_speedlimit.parquet")
            .exists());
        assert!(calib_out_dir.join("conflated_edges.parquet").exists());
        let conflated_csv = fs::read_to_string(calib_out_dir.join("conflated_edges.csv")).unwrap();
        let mut rdr = csv::Reader::from_reader(conflated_csv.as_bytes());
        for rec in rdr.deserialize::<bambam_congestion::ConflatedEdgeSummaryRow>() {
            let row = rec.unwrap();
            assert!(
                row.pop_density_1000_per_sq_mi > 0.0,
                "expected non-zero pop_density from boulder_co/census_tracts_bamcensus.csv.gz"
            );
            assert!(
                row.emp_density_1000_per_sq_mi > 0.0,
                "expected non-zero emp_density from boulder_co/census_tracts_bamcensus.csv.gz"
            );
        }

        // 5. Write toy XGBoost JSON models into `artifacts.models_dir` and run Imputation from the SAME unified config
        let model_with_sl_path = models_dir.join("xgboost_model_with_speedlimit.json");
        let model_without_sl_path = models_dir.join("xgboost_model_without_speedlimit.json");
        let toy_xgb_json = r#"{
            "learner": {
                "learner_model_param": {
                    "base_score": "[30.0]",
                    "num_feature": "16"
                },
                "gradient_booster": {
                    "model": {
                        "trees": [
                            {
                                "left_children": [1, -1, -1],
                                "right_children": [2, -1, -1],
                                "split_indices": [3, 0, 0],
                                "split_conditions": [0.5, 4.0, -2.0],
                                "default_left": [1, 0, 0]
                            }
                        ]
                    }
                }
            }
        }"#;
        fs::write(&model_with_sl_path, toy_xgb_json).unwrap();
        fs::write(&model_without_sl_path, toy_xgb_json).unwrap();

        let impute_cfg = loaded_pipeline.into_imputation_config();
        let impute_summary = run_imputation(&impute_cfg).expect("imputation failed");
        assert_eq!(impute_summary.total_edges, 3);
        assert_eq!(impute_summary.high_functional_edges, 2);
        assert_eq!(impute_summary.low_functional_edges, 1);

        for bin in TimeBinSpec::default_six_bins() {
            assert!(open_drive_dir
                .join(bin.enumerated_speed_filename())
                .exists());
        }
        assert!(open_drive_dir.join("predicted-speeds.parquet").exists());
        assert!(open_drive_dir.join("predicted-speeds.csv").exists());

        let _ = fs::remove_dir_all(tmp_root);
    }
}
