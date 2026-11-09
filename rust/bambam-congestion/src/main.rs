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
        run_calibration, run_imputation, CalibrationConfig, CalibrationOutputConfig,
        ConflationConfig, ImputationConfig, ImputationModelConfig, OpenNetworkConfig,
        PeriodSpeedFilesConfig, SpatialFeaturesConfig, TimePeriod, TomTomNetworkConfig,
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
    fn test_end_to_end_calibrate_and_impute() {
        let tmp_root = Path::new("target/test_tmp_congestion");
        let _ = fs::remove_dir_all(tmp_root);
        let open_drive_dir = tmp_root.join("open_net").join("drive");
        let tomtom_dir = tmp_root.join("tomtom_net");
        let calib_out_dir = tmp_root.join("calib_out");
        let models_dir = tmp_root.join("models");
        fs::create_dir_all(&open_drive_dir).unwrap();
        fs::create_dir_all(&tomtom_dir).unwrap();
        fs::create_dir_all(&calib_out_dir).unwrap();
        fs::create_dir_all(&models_dir).unwrap();

        // 1. Create a 3-edge synthetic Open Network:
        //    - edge 0: primary road with posted speed 35 mph (Eastbound)
        //    - edge 1: motorway_link (Link class -> forces without_speedlimit model) with posted speed 45 mph (Eastbound)
        //    - edge 2: residential road with missing posted speed (low-functional -> uses sl-med-imp floor = 15 mph)
        write_gz_lines(
            &open_drive_dir.parent().unwrap().join("vertices-complete.csv.gz"),
            &[
                "vertex_id,connector_id,x,y,traffic_signals,stop,give_way,crossing,infrastructure_ids",
                "0,n0,-105.0000,40.0000,true,false,false,true,sig1",
                "1,n1,-104.9980,40.0000,false,true,false,false,stp1",
                "2,n2,-104.9960,40.0000,false,false,false,false,",
                "3,n3,-104.9940,40.0000,false,false,false,false,",
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
                "LINESTRING(-105.0000 40.0000,-104.9980 40.0000)",
                "LINESTRING(-104.9980 40.0000,-104.9960 40.0000)",
                "LINESTRING(-104.9960 40.0000,-104.9940 40.0000)",
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

        // 2. Create synthetic TomTom network matching edges 0 and 1 (within 2m north, same Eastbound heading)
        write_gz_lines(
            &tomtom_dir.join("edges-geometries-enumerated.txt.gz"),
            &[
                "LINESTRING(-105.0000 40.00001,-104.9980 40.00001)",
                "LINESTRING(-104.9980 40.00001,-104.9960 40.00001)",
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
            write_gz_lines(
                &tomtom_dir.join(format!("tomtom-natl-{p_str}.txt.gz")),
                vals,
            );
        }

        // 3. Run Calibration
        let calib_cfg = CalibrationConfig {
            open_network: OpenNetworkConfig {
                network_dir: open_drive_dir.parent().unwrap().to_string_lossy().into_owned(),
                mode: "drive".to_string(),
                ..Default::default()
            },
            tomtom_network: TomTomNetworkConfig {
                network_dir: tomtom_dir.to_string_lossy().into_owned(),
                require_time_varying_trace: true,
                period_speed_files: PeriodSpeedFilesConfig::from_directory(
                    &tomtom_dir,
                    "tomtom-natl",
                ),
                ..Default::default()
            },
            conflation: ConflationConfig::default(),
            spatial_features: SpatialFeaturesConfig::default(),
            output: CalibrationOutputConfig {
                output_dir: calib_out_dir.to_string_lossy().into_owned(),
                write_parquet: true,
                write_csv: true,
            },
        };

        let calib_summary = run_calibration(&calib_cfg).expect("calibration failed");
        assert_eq!(calib_summary.total_open_edges, 3);
        assert_eq!(calib_summary.high_functional_edges, 2);
        assert_eq!(calib_summary.conflated_edges, 2);
        // Edge 0 (primary with speed limit) -> 6 time periods in with_speedlimit
        assert_eq!(calib_summary.samples_with_speedlimit, 6);
        // Edge 1 (motorway_link -> Link masks speed limit to None) -> 6 time periods in without_speedlimit
        assert_eq!(calib_summary.samples_without_speedlimit, 6);
        assert!(calib_out_dir.join("calibration_with_speedlimit.parquet").exists());
        assert!(calib_out_dir
            .join("calibration_without_speedlimit.parquet")
            .exists());
        assert!(calib_out_dir.join("conflated_edges.parquet").exists());

        // 4. Write toy XGBoost JSON models and run Imputation
        let model_with_sl_path = models_dir.join("with_sl.json");
        let model_without_sl_path = models_dir.join("without_sl.json");
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

        let impute_cfg = ImputationConfig {
            open_network: OpenNetworkConfig {
                network_dir: open_drive_dir.parent().unwrap().to_string_lossy().into_owned(),
                mode: "drive".to_string(),
                ..Default::default()
            },
            spatial_features: SpatialFeaturesConfig::default(),
            models: ImputationModelConfig {
                model_with_speedlimit_path: model_with_sl_path.to_string_lossy().into_owned(),
                model_without_speedlimit_path: model_without_sl_path.to_string_lossy().into_owned(),
            },
            output_dir: None,
            write_parquet_summary: true,
            write_csv_summary: true,
        };

        let impute_summary = run_imputation(&impute_cfg).expect("imputation failed");
        assert_eq!(impute_summary.total_edges, 3);
        assert_eq!(impute_summary.high_functional_edges, 2);
        assert_eq!(impute_summary.low_functional_edges, 1);

        for period in TimePeriod::ALL {
            assert!(open_drive_dir
                .join(period.enumerated_speed_filename())
                .exists());
        }
        assert!(open_drive_dir.join("predicted-speeds.parquet").exists());
        assert!(open_drive_dir.join("predicted-speeds.csv").exists());

        let _ = fs::remove_dir_all(tmp_root);
    }
}

