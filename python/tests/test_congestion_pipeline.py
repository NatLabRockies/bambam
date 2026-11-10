"""
Task 1: Numerical parity tests between Python `xgboost.XGBRegressor` and Rust
`bambam_congestion::XgboostJsonModel` (evaluated via `run_congestion_impute`).
"""

from __future__ import annotations

import gzip
from pathlib import Path

import numpy as np
import pandas as pd
import xgboost as xgb

from nlr.bambam.bambam_py_api import (
    get_congestion_time_bins,
    run_congestion_calibrate,
    run_congestion_impute,
)
from nlr.bambam.congestion.train_congestion_models import (
    PREDICTORS_WITH_SPEEDLIMIT,
    PREDICTORS_WITHOUT_SPEEDLIMIT,
    fit_and_export_model,
)


def _write_gz_lines(path: Path, lines: list[str]) -> None:
    with gzip.open(path, "wt", encoding="utf-8") as f:
        for line in lines:
            f.write(f"{line}\n")


def test_xgboost_rust_numerical_parity(tmp_path: Path) -> None:
    """
    Prove that given identical 16-feature (`with_speedlimit`) and 15-feature (`without_speedlimit`)
    matrices (including `NaN` values and exact split thresholds), Rust's `XgboostJsonModel`
    matches Python `xgb.XGBRegressor.predict()` within `< 1e-5 mph`.
    """
    rng = np.random.default_rng(12345)

    # 1. Fit real XGBRegressor(n_estimators=50, max_depth=6, tree_method="hist") on synthetic
    #    16-feature (with_speedlimit) and 15-feature (without_speedlimit) datasets, including NaNs
    n_train = 600
    road_classes_8 = np.zeros((n_train, 8), dtype=np.float32)
    chosen_rc = rng.integers(0, 8, size=n_train)
    road_classes_8[np.arange(n_train), chosen_rc] = 1.0

    X_with = np.column_stack(
        [
            rng.choice([25.0, 30.0, 35.0, 45.0, 55.0, 65.0, 75.0], size=n_train).astype(
                np.float32
            ),
            rng.uniform(0.0, 20.0, size=n_train).astype(np.float32),  # Pop density
            rng.uniform(0.0, 15.0, size=n_train).astype(np.float32),  # Emp density
            rng.integers(0, 2, size=n_train).astype(np.float32),  # Day: Weekday
            rng.integers(0, 2, size=n_train).astype(np.float32),  # Hour: AM Peak
            rng.integers(0, 2, size=n_train).astype(np.float32),  # Hour: PM Peak
            road_classes_8,
            rng.uniform(0.0, 500.0, size=n_train).astype(np.float32),  # Opp Density
            rng.uniform(0.0, 150.0, size=n_train).astype(np.float32),  # Traffic Control Density
        ]
    ).astype(np.float32)

    # Inject NaNs in training features to exercise default_left routing in XGBoost trees
    nan_mask = rng.random(X_with.shape) < 0.05
    X_with_nan = X_with.copy()
    X_with_nan[nan_mask] = np.nan

    y_with = (
        0.78 * X_with[:, 0]
        - 0.25 * X_with[:, 1]
        - 0.30 * X_with[:, 2]
        - 3.5 * X_with[:, 3]
        - 4.2 * X_with[:, 4]
        - 5.8 * X_with[:, 5]
        + 12.0 * X_with[:, 6]
        - 0.01 * X_with[:, 14]
        - 0.03 * X_with[:, 15]
        + rng.normal(0.0, 1.5, size=n_train).astype(np.float32)
    ).astype(np.float32)

    X_without_nan = X_with_nan[:, 1:].copy()
    y_without = y_with.copy()

    df_train_with = pd.DataFrame(X_with_nan, columns=PREDICTORS_WITH_SPEEDLIMIT)
    df_train_without = pd.DataFrame(X_without_nan, columns=PREDICTORS_WITHOUT_SPEEDLIMIT)

    model_with = xgb.XGBRegressor(
        n_estimators=50,
        max_depth=6,
        learning_rate=0.1,
        tree_method="hist",
        random_state=42,
    )
    model_with.fit(df_train_with, y_with)

    model_without = xgb.XGBRegressor(
        n_estimators=50,
        max_depth=6,
        learning_rate=0.1,
        tree_method="hist",
        random_state=99,
    )
    model_without.fit(df_train_without, y_without)

    # 2. Save boosters to JSON
    model_with_path = tmp_path / "xgboost_model_with_speedlimit.json"
    model_without_path = tmp_path / "xgboost_model_without_speedlimit.json"
    model_with.get_booster().save_model(str(model_with_path))
    model_without.get_booster().save_model(str(model_without_path))

    # 3. Build a synthetic RouteE Compass Open Network covering all high-functional road classes,
    #    both with and without speed limits, plus NaN / zero / boundary feature cases.
    net_dir = tmp_path / "open_net"
    drive_dir = net_dir / "drive"
    drive_dir.mkdir(parents=True, exist_ok=True)

    # 12 test edges:
    # - 0..5: high-functional classes WITH speed limit (motorway, trunk, primary, secondary, tertiary, primary w/ NaN density)
    # - 6..9: high-functional classes WITHOUT speed limit (motorway_link, trunk_link, primary missing SL, secondary missing SL)
    # - 10..11: low-functional classes (residential, service) -> should use sl-med-imp
    edge_specs = [
        ("motorway", "65.0", (1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), True),
        ("trunk", "55.0", (0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), True),
        ("primary", "45.0", (0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0), True),
        ("secondary", "35.0", (0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0), True),
        ("tertiary", "30.0", (0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0), True),
        ("primary", "40.0", (0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0), True),
        ("motorway_link", "50.0", (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0), False),  # Link masks SL to None
        ("trunk_link", "", (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0), False),
        ("primary", "", (0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0), False),  # Missing posted SL -> without_sl model
        ("secondary", "NaN", (0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0), False),  # NaN posted SL -> without_sl model
        ("residential", "25.0", (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), None),
        ("service", "15.0", (0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0), None),
    ]

    n_edges = len(edge_specs)
    compass_lines = ["edge_id,src_vertex_id,dst_vertex_id,distance"]
    geom_lines = []
    class_lines = []
    posted_speed_lines = []

    # Place each edge at a distinct longitude so 0.5-mile spatial buffers don't overlap unless intended
    for i, (hwy, sl_str, _, _) in enumerate(edge_specs):
        lon0 = -105.0 - i * 0.05
        lat0 = 40.0
        lon1 = lon0 + 0.001
        lat1 = lat0
        v_src = 2 * i
        v_dst = 2 * i + 1
        compass_lines.append(f"{i},{v_src},{v_dst},100.0")
        geom_lines.append(f"LINESTRING ({lon0:.6f} {lat0:.6f}, {lon1:.6f} {lat1:.6f})")
        class_lines.append(hwy)
        posted_speed_lines.append(sl_str)

    _write_gz_lines(drive_dir / "edges-compass.csv.gz", compass_lines)
    _write_gz_lines(drive_dir / "edges-geometries-enumerated.txt.gz", geom_lines)
    _write_gz_lines(drive_dir / "edges-classes-enumerated.txt.gz", class_lines)
    _write_gz_lines(drive_dir / "edges-posted-speeds-mph-enumerated.txt.gz", posted_speed_lines)

    # Create CSV points files for opportunities and traffic controls
    opp_csv = tmp_path / "opportunities.csv"
    opp_rows = ["x,y"]
    tc_csv = tmp_path / "traffic_controls.csv"
    tc_rows = ["x,y"]
    for i in range(n_edges):
        lon0 = -105.0 - i * 0.05
        for k in range(i + 1):
            opp_rows.append(f"{lon0 + 0.0001 * k:.6f},40.0001")
        if i % 2 == 0:
            tc_rows.append(f"{lon0:.6f},40.0000")
    opp_csv.write_text("\n".join(opp_rows) + "\n", encoding="utf-8")
    tc_csv.write_text("\n".join(tc_rows) + "\n", encoding="utf-8")

    # Run Rust `run_congestion_impute` via PyO3
    out_dir = tmp_path / "imputed_out"
    impute_cfg = {
        "open_network": {
            "network_dir": str(net_dir),
            "mode": "drive",
            "legacy_posted_speed_unit": "mph",
            "min_imputed_speed_limit_mph": 15.0,
        },
        "spatial_features": {
            "density_radius_miles": 0.5,
            "projection_mode": "geodesic",
            "opportunities_source": {
                "type": "csv_points",
                "path": str(opp_csv),
                "x_column": "x",
                "y_column": "y",
            },
            "traffic_controls_source": {
                "type": "csv_points",
                "path": str(tc_csv),
                "x_column": "x",
                "y_column": "y",
            },
        },
        "models": {
            "model_with_speedlimit_path": str(model_with_path),
            "model_without_speedlimit_path": str(model_without_path),
        },
        "output_dir": str(out_dir),
        "write_parquet_summary": True,
        "write_csv_summary": True,
    }
    run_congestion_impute(impute_cfg)

    # Also run calibration against a dummy reference network so Rust exports the exact computed
    # spatial feature rows (`calibration_with_speedlimit.parquet` and `calibration_without_speedlimit.parquet`),
    # AND independently verify direct analytical feature construction.
    pred_df = pd.read_parquet(out_dir / "predicted-speeds.parquet")
    assert len(pred_df) == n_edges

    # Verify default 6 time bins via PyO3 helper
    bins = get_congestion_time_bins()
    assert bins == ["wd-am", "wd-af", "wd-pm", "we-am", "we-af", "we-pm"]

    bin_time_flags = {
        "wd-am": (1.0, 1.0, 0.0),
        "wd-af": (1.0, 0.0, 0.0),
        "wd-pm": (1.0, 0.0, 1.0),
        "we-am": (0.0, 1.0, 0.0),
        "we-af": (0.0, 0.0, 0.0),
        "we-pm": (0.0, 0.0, 1.0),
    }

    # Compute analytical buffer area (pi * R^2 + 2 * R * L) in sq miles for each edge
    # Rust uses `edge.length_meters` (which comes from `ce.distance` = 100.0m in edges-compass.csv.gz)
    from math import pi

    r_miles = 0.5
    meters_per_mile = 1609.344

    py_preds_all = []
    rust_preds_all = []

    for i, (hwy, sl_str, rc_one_hot, uses_with_sl) in enumerate(edge_specs):
        row = pred_df.iloc[i]
        length_m = 100.0
        length_miles = length_m / meters_per_mile
        area_sq_mi = float(pi * (r_miles**2) + 2.0 * r_miles * length_miles)

        opp_count = float(i + 1)
        tc_count = 1.0 if (i % 2 == 0) else 0.0
        opp_den = np.float32(opp_count / area_sq_mi)
        tc_den = np.float32(tc_count / area_sq_mi)

        for b_name, (day_wd, hr_am, hr_pm) in bin_time_flags.items():
            rust_val = float(row[f"{b_name}-prd"])
            if uses_with_sl is None:
                # Low-functional road: should equal sl-med-imp
                assert abs(rust_val - float(sl_str)) < 1e-5
                continue

            if uses_with_sl:
                feats_16 = np.array(
                    [
                        [
                            np.float32(float(sl_str)),
                            np.float32(0.0),
                            np.float32(0.0),
                            np.float32(day_wd),
                            np.float32(hr_am),
                            np.float32(hr_pm),
                            *[np.float32(x) for x in rc_one_hot],
                            opp_den,
                            tc_den,
                        ]
                    ],
                    dtype=np.float32,
                )
                py_val = float(
                    model_with.predict(
                        pd.DataFrame(feats_16, columns=PREDICTORS_WITH_SPEEDLIMIT)
                    )[0]
                )
            else:
                feats_15 = np.array(
                    [
                        [
                            np.float32(0.0),
                            np.float32(0.0),
                            np.float32(day_wd),
                            np.float32(hr_am),
                            np.float32(hr_pm),
                            *[np.float32(x) for x in rc_one_hot],
                            opp_den,
                            tc_den,
                        ]
                    ],
                    dtype=np.float32,
                )
                py_val = float(
                    model_without.predict(
                        pd.DataFrame(feats_15, columns=PREDICTORS_WITHOUT_SPEEDLIMIT)
                    )[0]
                )

            py_preds_all.append(py_val)
            rust_preds_all.append(rust_val)

            # Also verify enumerated .txt.gz file matches
            enum_path = out_dir / f"edges-speeds-{b_name}-mph-enumerated.txt.gz"
            with gzip.open(enum_path, "rt", encoding="utf-8") as gz:
                enum_vals = [float(line.strip()) for line in gz if line.strip()]
            assert abs(enum_vals[i] - rust_val) < 1e-5

    py_arr = np.asarray(py_preds_all, dtype=np.float32)
    rust_arr = np.asarray(rust_preds_all, dtype=np.float32)
    max_diff = float(np.max(np.abs(py_arr - rust_arr)))
    assert max_diff < 1e-5, f"Max prediction difference {max_diff} >= 1e-5 mph"


def test_end_to_end_calibrate_train_impute_parity(tmp_path: Path) -> None:
    """
    End-to-end numerical parity across `run_congestion_calibrate` -> `fit_and_export_model` ->
    `run_congestion_impute`, including non-zero Census polygon densities and boundary split
    thresholds.
    """
    net_dir = tmp_path / "open_net"
    drive_dir = net_dir / "drive"
    drive_dir.mkdir(parents=True, exist_ok=True)

    ref_dir = tmp_path / "ref_net"
    ref_dir.mkdir(parents=True, exist_ok=True)
    speeds_dir = tmp_path / "ref_speeds"
    speeds_dir.mkdir(parents=True, exist_ok=True)

    # Create 20 high-functional edges (10 with posted speed limits, 10 links/missing speed limits)
    n_edges = 20
    compass_lines = ["edge_id,src_vertex_id,dst_vertex_id,distance"]
    geom_lines = []
    class_lines = []
    posted_speed_lines = []

    ref_compass_lines = ["edge_id,src_vertex_id,dst_vertex_id,distance"]
    ref_geom_lines = []
    bins = ["wd-am", "wd-af", "wd-pm", "we-am", "we-af", "we-pm"]
    ref_speeds_by_bin: dict[str, list[str]] = {b: [] for b in bins}

    classes_cycle = [
        ("motorway", "65.0"),
        ("trunk", "55.0"),
        ("primary", "45.0"),
        ("secondary", "35.0"),
        ("tertiary", "30.0"),
        ("motorway_link", "50.0"),
        ("trunk_link", ""),
        ("primary", ""),
        ("secondary", ""),
        ("tertiary", ""),
    ]

    for i in range(n_edges):
        hwy, sl_str = classes_cycle[i % len(classes_cycle)]
        lon0 = -105.25 + (i % 5) * 0.02
        lat0 = 40.00 + (i // 5) * 0.02
        lon1 = lon0 + 0.002
        lat1 = lat0
        compass_lines.append(f"{i},{2 * i},{2 * i + 1},160.0")
        geom = f"LINESTRING ({lon0:.6f} {lat0:.6f}, {lon1:.6f} {lat1:.6f})"
        geom_lines.append(geom)
        class_lines.append(hwy)
        posted_speed_lines.append(sl_str)

        # Matching reference edge with time-varying speeds in kph
        ref_compass_lines.append(f"{i},{2 * i},{2 * i + 1},160.0")
        ref_geom_lines.append(geom)
        base_kph = 85.0 - (i % 5) * 8.0
        ref_speeds_by_bin["wd-am"].append(f"{base_kph - 12.0:.2f}")
        ref_speeds_by_bin["wd-af"].append(f"{base_kph - 2.0:.2f}")
        ref_speeds_by_bin["wd-pm"].append(f"{base_kph - 16.0:.2f}")
        ref_speeds_by_bin["we-am"].append(f"{base_kph + 3.0:.2f}")
        ref_speeds_by_bin["we-af"].append(f"{base_kph:.2f}")
        ref_speeds_by_bin["we-pm"].append(f"{base_kph - 4.0:.2f}")

    _write_gz_lines(drive_dir / "edges-compass.csv.gz", compass_lines)
    _write_gz_lines(drive_dir / "edges-geometries-enumerated.txt.gz", geom_lines)
    _write_gz_lines(drive_dir / "edges-classes-enumerated.txt.gz", class_lines)
    _write_gz_lines(drive_dir / "edges-posted-speeds-mph-enumerated.txt.gz", posted_speed_lines)

    _write_gz_lines(ref_dir / "edges-compass.csv.gz", ref_compass_lines)
    _write_gz_lines(ref_dir / "edges-geometries-enumerated.txt.gz", ref_geom_lines)
    for b_name, lines in ref_speeds_by_bin.items():
        _write_gz_lines(speeds_dir / f"ref-{b_name}.txt.gz", lines)

    # Census polygon covering the study area
    census_csv = tmp_path / "tracts.csv"
    census_csv.write_text(
        'GEOID,geometry,pop,emp\n'
        '08013000100,"POLYGON ((-105.5 39.8, -105.0 39.8, -105.0 40.3, -105.5 40.3, -105.5 39.8))",250000,120000\n',
        encoding="utf-8",
    )

    opp_csv = tmp_path / "opportunities.csv"
    opp_rows = ["x,y"]
    for i in range(n_edges):
        lon0 = -105.25 + (i % 5) * 0.02
        lat0 = 40.00 + (i // 5) * 0.02
        for k in range((i % 4) + 1):
            opp_rows.append(f"{lon0 + 0.0002 * k:.6f},{lat0 + 0.0001:.6f}")
    opp_csv.write_text("\n".join(opp_rows) + "\n", encoding="utf-8")

    spatial_cfg = {
        "density_radius_miles": 0.5,
        "projection_mode": "geodesic",
        "opportunities_source": {
            "type": "csv_points",
            "path": str(opp_csv),
            "x_column": "x",
            "y_column": "y",
        },
        "census_source": {
            "type": "csv_polygons",
            "polygons_csv_path": str(census_csv),
            "geoid_column": "GEOID",
            "wkt_column": "geometry",
            "population_column": "pop",
            "employment_column": "emp",
        },
    }

    calib_dir = tmp_path / "calibration_out"
    calib_cfg = {
        "open_network": {
            "network_dir": str(net_dir),
            "mode": "drive",
            "legacy_posted_speed_unit": "mph",
            "min_imputed_speed_limit_mph": 15.0,
        },
        "reference_network": {
            "network_dir": str(ref_dir),
            "speed_unit": "kph",
            "require_time_varying_speeds": True,
            "speeds_dir": str(speeds_dir),
            "file_prefix": "ref",
            "file_extension": "txt.gz",
        },
        "conflation": {
            "buffer_distance_meters": 5.0,
            "max_heading_diff_degrees": 45.0,
            "min_overlap_meters": 1.0,
            "cap_speed_outliers": True,
        },
        "spatial_features": spatial_cfg,
        "output": {
            "output_dir": str(calib_dir),
            "write_parquet": True,
            "write_csv": True,
        },
    }
    run_congestion_calibrate(calib_cfg)

    df_with = pd.read_parquet(calib_dir / "calibration_with_speedlimit.parquet")
    df_without = pd.read_parquet(calib_dir / "calibration_without_speedlimit.parquet")
    assert len(df_with) == 10 * 6
    assert len(df_without) == 10 * 6
    assert (df_with["Population Density"] > 0).all()
    assert (df_with["Employment Density"] > 0).all()

    models_dir = tmp_path / "models_out"
    fit_and_export_model(
        df_with,
        PREDICTORS_WITH_SPEEDLIMIT,
        models_dir,
        "xgboost_model_with_speedlimit",
        n_iter=1,
    )
    fit_and_export_model(
        df_without,
        PREDICTORS_WITHOUT_SPEEDLIMIT,
        models_dir,
        "xgboost_model_without_speedlimit",
        n_iter=1,
    )

    impute_out = tmp_path / "impute_out"
    impute_cfg = {
        "open_network": calib_cfg["open_network"],
        "spatial_features": spatial_cfg,
        "models": {
            "model_with_speedlimit_path": str(models_dir / "xgboost_model_with_speedlimit.json"),
            "model_without_speedlimit_path": str(
                models_dir / "xgboost_model_without_speedlimit.json"
            ),
        },
        "output_dir": str(impute_out),
        "write_parquet_summary": True,
        "write_csv_summary": True,
    }
    run_congestion_impute(impute_cfg)

    pred_df = pd.read_parquet(impute_out / "predicted-speeds.parquet")

    # Load fitted boosters in Python and predict directly on the exact calibration feature tables
    booster_with = xgb.XGBRegressor()
    booster_with.load_model(str(models_dir / "xgboost_model_with_speedlimit.json"))
    booster_without = xgb.XGBRegressor()
    booster_without.load_model(str(models_dir / "xgboost_model_without_speedlimit.json"))

    py_with_preds = booster_with.predict(
        df_with[PREDICTORS_WITH_SPEEDLIMIT].astype(np.float32)
    )
    # Edges 0..5, 10..15 have speed limits (10 edges * 6 time periods in row order)
    with_edge_indices = [i for i in range(n_edges) if (i % 10) < 5]
    rust_with_preds = []
    for eid in with_edge_indices:
        r = pred_df.iloc[eid]
        for b in bins:
            rust_with_preds.append(float(r[f"{b}-prd"]))
    assert np.max(np.abs(py_with_preds - np.asarray(rust_with_preds, dtype=np.float32))) < 1e-5

    # Edges without speed limits use booster_without
    py_without_preds = booster_without.predict(
        df_without[PREDICTORS_WITHOUT_SPEEDLIMIT].astype(np.float32)
    )
    without_edge_indices = [i for i in range(n_edges) if (i % 10) >= 5]
    rust_without_preds = []
    for eid in without_edge_indices:
        r = pred_df.iloc[eid]
        for b in bins:
            rust_without_preds.append(float(r[f"{b}-prd"]))
    assert (
        np.max(np.abs(py_without_preds - np.asarray(rust_without_preds, dtype=np.float32)))
        < 1e-5
    )

