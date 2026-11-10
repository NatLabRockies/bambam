"""
Task 3: Domain & Statistical Validation Report for `bambam-congestion`.

Consumes `calibration_with_speedlimit.parquet`, `calibration_without_speedlimit.parquet`,
`conflated_edges.parquet`, and `predicted-speeds.parquet` (plus the enumerated speed files
in `<imputation_output_dir>/edges-speeds-{bin}-mph-enumerated.txt.gz`) and produces:

1. **Out-of-Sample Accuracy Table:** `RMSE`, `MAE`, and `SMAPE` overall and broken down by
   `CongestionRoadClass` and `TimeBinSpec` (using the exact 80/20 panel split by link ID).
2. **Monotonicity & Peak-Dip Check:** Median and mean predicted speed by Road Class x Time Bin
   (`Motorway > Trunk > Primary > Secondary > Tertiary > Residential > Service`, with weekday
   peak periods `wd-am` and `wd-pm` lower than weekend periods `we-am`, `we-af`, `we-pm`).
3. **RouteE Compass Routing Smoke Test:** Runs `BambamRunner` (RouteE Compass) searches across
   origins in the network using `edges-speeds-wd-am-mph-enumerated.txt.gz`,
   `edges-speeds-wd-pm-mph-enumerated.txt.gz`, and `edges-speeds-we-af-mph-enumerated.txt.gz`
   and verifies that peak-hour drive isochrones contract relative to off-peak isochrones.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import tempfile
import tomllib
from typing import Any

import numpy as np
import pandas as pd
from shapely import wkb
from sklearn.metrics import mean_absolute_error, mean_squared_error
from sklearn.model_selection import train_test_split
import xgboost as xgb

from nlr.bambam.bambam_app import BambamRunner
from nlr.bambam.congestion.train_congestion_models import (
    PREDICTORS_WITH_SPEEDLIMIT,
    PREDICTORS_WITHOUT_SPEEDLIMIT,
    load_table,
)

ROAD_CLASS_ONEHOT_COLS: list[tuple[str, str]] = [
    ("Motorway", "Road Class: Motorway"),
    ("Trunk", "Road Class: Trunk"),
    ("Primary", "Road Class: Primary"),
    ("Secondary", "Road Class: Secondary"),
    ("Tertiary", "Road Class: Tertiary"),
    ("Service", "Road Class: Service"),
    ("Link", "Road Class: Link"),
    ("Other", "Road Class: Other"),
]


def _compute_metrics(y_true: np.ndarray, y_pred: np.ndarray) -> dict[str, float]:
    rmse = float(np.sqrt(mean_squared_error(y_true, y_pred)))
    mae = float(mean_absolute_error(y_true, y_pred))
    denom = np.abs(y_true) + np.abs(y_pred)
    smape = float(
        np.mean(np.where(denom > 0, 2.0 * np.abs(y_true - y_pred) / denom * 100.0, 0.0))
    )
    return {
        "n_samples": int(len(y_true)),
        "obs_mean_mph": float(np.mean(y_true)),
        "pred_mean_mph": float(np.mean(y_pred)),
        "rmse_mph": rmse,
        "mae_mph": mae,
        "smape_pct": smape,
    }


def _decode_road_class(df: pd.DataFrame) -> pd.Series:
    rc = pd.Series("Residential", index=df.index, dtype="object")
    for label, col in ROAD_CLASS_ONEHOT_COLS:
        if col in df.columns:
            rc = np.where(df[col] > 0.5, label, rc)
    return pd.Series(rc, index=df.index)


def evaluate_out_of_sample_accuracy(
    calib_dir: Path,
    models_dir: Path,
    random_state: int = 42,
) -> tuple[pd.DataFrame, pd.DataFrame, pd.DataFrame]:
    """
    Reconstructs the exact 80/20 panel test split by link ID (`random_state=42`) for both
    `with_speedlimit` and `without_speedlimit` tables, evaluates the exported `.json` models,
    and returns `(overall_df, by_road_class_df, by_time_bin_df)`.
    """
    test_frames: list[pd.DataFrame] = []
    overall_rows: list[dict[str, Any]] = []

    specs = [
        (
            "with_speedlimit",
            calib_dir / "calibration_with_speedlimit.parquet",
            models_dir / "xgboost_model_with_speedlimit.json",
            PREDICTORS_WITH_SPEEDLIMIT,
        ),
        (
            "without_speedlimit",
            calib_dir / "calibration_without_speedlimit.parquet",
            models_dir / "xgboost_model_without_speedlimit.json",
            PREDICTORS_WITHOUT_SPEEDLIMIT,
        ),
    ]

    for model_name, table_path, model_path, predictors in specs:
        if not table_path.exists() or not model_path.exists():
            continue
        df = load_table(table_path).dropna(subset=["Speed"]).reset_index(drop=True)
        id_col = "ID" if "ID" in df.columns else "edge_id"
        unique_ids = df[id_col].unique()
        _, test_ids = train_test_split(
            unique_ids, train_size=0.8, random_state=random_state
        )
        test_df = df[df[id_col].isin(test_ids)].copy()

        booster = xgb.XGBRegressor()
        booster.load_model(str(model_path))
        preds = booster.predict(test_df[list(predictors)].astype(np.float32))
        test_df["Predicted_Speed"] = preds
        test_df["Road_Class"] = _decode_road_class(test_df)
        test_df["Model_Group"] = model_name
        test_frames.append(test_df)

        m = _compute_metrics(
            test_df["Speed"].to_numpy(dtype=np.float32),
            preds.astype(np.float32),
        )
        overall_rows.append({"model": model_name, **m})

    combined_test = pd.concat(test_frames, ignore_index=True)
    m_all = _compute_metrics(
        combined_test["Speed"].to_numpy(dtype=np.float32),
        combined_test["Predicted_Speed"].to_numpy(dtype=np.float32),
    )
    overall_rows.append({"model": "combined_all_test", **m_all})
    overall_df = pd.DataFrame(overall_rows)

    rc_order = ["Motorway", "Trunk", "Primary", "Secondary", "Tertiary", "Link"]
    rc_rows: list[dict[str, Any]] = []
    for rc in rc_order:
        sub = combined_test[combined_test["Road_Class"] == rc]
        if len(sub) == 0:
            continue
        m = _compute_metrics(
            sub["Speed"].to_numpy(dtype=np.float32),
            sub["Predicted_Speed"].to_numpy(dtype=np.float32),
        )
        rc_rows.append({"road_class": rc, **m})
    by_road_class_df = pd.DataFrame(rc_rows)

    bin_order = ["wd-am", "wd-af", "wd-pm", "we-am", "we-af", "we-pm"]
    bin_rows: list[dict[str, Any]] = []
    for b in bin_order:
        sub = combined_test[combined_test["period"] == b]
        if len(sub) == 0:
            continue
        m = _compute_metrics(
            sub["Speed"].to_numpy(dtype=np.float32),
            sub["Predicted_Speed"].to_numpy(dtype=np.float32),
        )
        bin_rows.append({"time_bin": b, **m})
    by_time_bin_df = pd.DataFrame(bin_rows)

    return overall_df, by_road_class_df, by_time_bin_df


def _map_highway_to_road_class(hwy: str) -> str:
    h = str(hwy).strip().lower()
    if h == "motorway":
        return "Motorway"
    if h == "trunk":
        return "Trunk"
    if h in ("motorway_link", "trunk_link"):
        return "Link"
    if h == "primary":
        return "Primary"
    if h == "secondary":
        return "Secondary"
    if h == "tertiary":
        return "Tertiary"
    if h == "residential":
        return "Residential"
    if h == "service":
        return "Service"
    return "Other"


def check_monotonicity_and_peak_dip(pred_df: pd.DataFrame) -> tuple[pd.DataFrame, dict[str, bool]]:
    """
    Computes median predicted operating speed by `CongestionRoadClass` x `TimeBinSpec`
    and verifies:
    - Functional class hierarchy: `Motorway > Trunk > Primary > Secondary > Tertiary >= Residential >= Service`
    - Peak-dip behavior: across high-functional classes (`Motorway`, `Trunk`, `Primary`, `Secondary`, `Tertiary`, `Link`),
      weekday evening peak (`wd-pm`) and weekday morning peak (`wd-am`) have lower speeds than weekend (`we-am`, `we-af`).
    """
    df = pred_df.copy()
    df["road_class"] = df["highway"].map(_map_highway_to_road_class)

    rc_order = [
        "Motorway",
        "Trunk",
        "Primary",
        "Secondary",
        "Tertiary",
        "Link",
        "Residential",
        "Service",
        "Other",
    ]
    prd_cols = ["wd-am-prd", "wd-af-prd", "wd-pm-prd", "we-am-prd", "we-af-prd", "we-pm-prd"]

    rows: list[dict[str, Any]] = []
    for rc in rc_order:
        sub = df[df["road_class"] == rc]
        if len(sub) == 0:
            continue
        row: dict[str, Any] = {
            "road_class": rc,
            "n_edges": int(len(sub)),
            "sl-med-imp": float(sub["sl-med-imp"].median()),
        }
        for c in prd_cols:
            row[f"{c}_median"] = float(sub[c].median())
            row[f"{c}_mean"] = float(sub[c].mean())
        rows.append(row)

    summary_df = pd.DataFrame(rows).set_index("road_class")

    # Verify hierarchy on main arterial/highway ladder: Motorway > Trunk > Primary > Secondary > Tertiary >= Residential
    ladder = [
        rc
        for rc in ["Motorway", "Trunk", "Primary", "Secondary", "Tertiary", "Residential"]
        if rc in summary_df.index
    ]
    monotonic_checks: dict[str, bool] = {}
    for c in prd_cols:
        vals = [float(summary_df.loc[rc, f"{c}_median"]) for rc in ladder]
        is_mono = all(vals[i] >= vals[i + 1] for i in range(len(vals) - 1)) and (
            vals[0] > vals[-1]
        )
        monotonic_checks[f"hierarchy_{c}"] = is_mono

    # Verify peak dip on high-functional classes: wd-pm < we-af and wd-am < we-am
    high_func_classes = [
        rc
        for rc in ["Motorway", "Trunk", "Primary", "Secondary", "Tertiary", "Link"]
        if rc in summary_df.index
    ]
    for rc in high_func_classes:
        wd_pm = float(summary_df.loc[rc, "wd-pm-prd_median"])
        wd_am = float(summary_df.loc[rc, "wd-am-prd_median"])
        we_af = float(summary_df.loc[rc, "we-af-prd_median"])
        we_am = float(summary_df.loc[rc, "we-am-prd_median"])
        monotonic_checks[f"peak_dip_{rc}_wd_pm_lt_we_af"] = wd_pm < we_af
        monotonic_checks[f"peak_dip_{rc}_wd_am_lt_we_am"] = wd_am < we_am

    return summary_df.reset_index(), monotonic_checks


def run_compass_routing_smoke_test(
    open_network_dir: Path,
    imputation_dir: Path,
    time_bins_minutes: list[int] | None = None,
) -> pd.DataFrame:
    """
    Runs a RouteE Compass (`BambamRunner`) routing smoke test comparing peak (`wd-am`, `wd-pm`)
    vs. off-peak (`we-af`) enumerated speed tables and verifies that peak-hour drive isochrones
    contract relative to off-peak isochrones.
    """
    if time_bins_minutes is None:
        time_bins_minutes = [2, 4, 6]
    max_time = float(max(time_bins_minutes))

    edges_file = (
        open_network_dir / "drive" / "edges-compass.csv.gz"
        if (open_network_dir / "drive" / "edges-compass.csv.gz").exists()
        else open_network_dir / "edges-compass.csv.gz"
    )
    geom_file = (
        open_network_dir / "drive" / "edges-geometries-enumerated.txt.gz"
        if (open_network_dir / "drive" / "edges-geometries-enumerated.txt.gz").exists()
        else open_network_dir / "edges-geometries-enumerated.txt.gz"
    )
    vertices_file = open_network_dir / "vertices-compass.csv.gz"

    # Select deterministic origins near the center of the network's vertex bounding box
    v_df = pd.read_csv(vertices_file)
    cx = float(v_df["x"].median())
    cy = float(v_df["y"].median())
    q25_x, q75_x = float(v_df["x"].quantile(0.35)), float(v_df["x"].quantile(0.65))
    q25_y, q75_y = float(v_df["y"].quantile(0.35)), float(v_df["y"].quantile(0.65))

    origins = [
        ("center", cx, cy),
        ("southwest_core", q25_x, q25_y),
        ("northeast_core", q75_x, q75_y),
    ]

    periods_to_compare = ["wd-am", "wd-pm", "we-af"]
    results_by_period: dict[str, list[dict[str, Any]]] = {}

    for period in periods_to_compare:
        speed_file = imputation_dir / f"edges-speeds-{period}-mph-enumerated.txt.gz"
        toml_str = f"""
[system]
parallelism = 2
response_persistence_policy = "persist_response_in_memory"

[graph]
edge_list = [{{ input_file = "{edges_file.as_posix()}" }}]
vertex_list_input_file = "{vertices_file.as_posix()}"

[label]
type = "multimodal"
max_trip_legs = 1
modes = ["drive"]

[mapping]
type = "edge"
geometry = [{{ type = "from_linestrings", geometry_input_file = "{geom_file.as_posix()}" }}]
tolerance.distance = 0.5
tolerance.unit = "kilometers"
queries_without_destinations = true
matching_type = ["point", "vertex_id", "edge_id"]

[algorithm]
type = "dijkstras"
termination_behavior = "allow_tree_termination"

[termination]
type = "solution_size"
limit = 200_000

[cost]
weights = {{ trip_time = 1.0 }}
vehicle_rates = {{ trip_time = {{ type = "raw" }} }}

[[search]]
traversal.type = "combined"
traversal.models = [
    {{ type = "distance", distance_unit = "miles" }},
    {{ type = "speed", speed_unit = "mph", speed_table_input_file = "{speed_file.as_posix()}" }},
    {{ type = "time", time_unit = "minutes" }},
    {{ type = "multimodal", this_mode = "drive", available_modes = ["drive"] }},
]
constraint.type = "combined"
constraint.models = [
    {{ type = "time_limit", time_limit = {{ time = {max_time}, time_unit = "minutes" }} }},
    {{ type = "multimodal", this_mode = "drive", available_modes = ["drive"] }},
]

[[plugin.output_plugins]]
type = "traversal"

[[plugin.output_plugins]]
type = "bambam"
format = "aggregate"
isochrone_format = "wkb"
binning.type = "global"
binning.value = {{ type = "time", feature = "trip_time", values = {time_bins_minutes}, unit = "minutes" }}
geometry_model.type = "global"
geometry_model.value = {{ type = "destination_point" }}
isochrone_algorithm.type = "global"
isochrone_algorithm.value = {{ type = "concave_hull", concavity = 2.0 }}

[[plugin.output_plugins]]
type = "isochrone"
"""
        with tempfile.NamedTemporaryFile("w", suffix=".toml", dir=".", delete=False) as tf:
            tf.write(toml_str)
            tf_path = Path(tf.name)
        try:
            app = BambamRunner.from_config_file(tf_path)
            queries = [
                {
                    "origin_x": ox,
                    "origin_y": oy,
                    "grid_id": label,
                    "trip_mode": "drive",
                    "max_trip_legs": 1,
                    "constraints": [
                        {"type": "time_limit", "limit": max_time, "unit": "minutes"},
                        {"type": "allowed_modes", "values": ["drive"]},
                    ],
                }
                for label, ox, oy in origins
            ]
            results_by_period[period] = app.run(queries)
        finally:
            tf_path.unlink(missing_ok=True)

    comparison_rows: list[dict[str, Any]] = []
    for idx, (label, ox, oy) in enumerate(origins):
        for tb in time_bins_minutes:
            tb_key = str(tb)
            row: dict[str, Any] = {
                "origin": label,
                "origin_x": round(ox, 5),
                "origin_y": round(oy, 5),
                "isochrone_min": tb,
            }
            for period in periods_to_compare:
                resp = results_by_period[period][idx]
                if "error" in resp:
                    raise RuntimeError(
                        f"RouteE Compass search failed for {label} ({period}): {resp['error']}"
                    )
                bin_data = resp["aggregate_opportunities"][tb_key]
                n_dest = int(bin_data["n_destinations"])
                iso_hex = bin_data.get("isochrone", "")
                area_km2 = 0.0
                if iso_hex:
                    geom = wkb.loads(bytes.fromhex(iso_hex))
                    # Convert WGS84 deg^2 at latitude oy to approximate km^2
                    cos_lat = float(np.cos(np.radians(oy)))
                    area_km2 = float(geom.area * (111.32 * cos_lat) * 110.57)
                row[f"{period}_destinations"] = n_dest
                row[f"{period}_area_km2"] = round(area_km2, 3)

            row["wd_pm_vs_we_af_dest_delta"] = (
                row["wd-pm_destinations"] - row["we-af_destinations"]
            )
            row["wd_am_vs_we_af_dest_delta"] = (
                row["wd-am_destinations"] - row["we-af_destinations"]
            )
            row["wd_pm_vs_we_af_area_pct"] = round(
                (row["wd-pm_area_km2"] - row["we-af_area_km2"])
                / max(row["we-af_area_km2"], 1e-6)
                * 100.0,
                2,
            )
            comparison_rows.append(row)

    comp_df = pd.DataFrame(comparison_rows)
    return comp_df


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Validate calibrated congestion models, imputed operating speeds, and RouteE Compass isochrones"
    )
    parser.add_argument(
        "-c",
        "--config",
        type=Path,
        default=Path("configuration/bambam-congestion/congestion.toml"),
        help="Path to unified congestion.toml",
    )
    parser.add_argument(
        "--report-out",
        type=Path,
        default=None,
        help="Optional path to save the Markdown validation report",
    )
    args = parser.parse_args()

    with args.config.open("rb") as f:
        cfg = tomllib.load(f)

    open_net_dir = Path(cfg["open_network"]["network_dir"])
    mode = cfg["open_network"].get("mode", "drive")
    artifacts = cfg.get("artifacts", {})
    calib_dir = Path(artifacts.get("calibration_dir", "output/congestion_calibration_boulder"))
    models_dir = Path(artifacts.get("models_dir", "output/congestion_models"))
    imputation_dir = Path(
        artifacts.get(
            "imputation_output_dir",
            str(open_net_dir / mode if (open_net_dir / mode).exists() else open_net_dir),
        )
    )

    overall_df, by_rc_df, by_bin_df = evaluate_out_of_sample_accuracy(calib_dir, models_dir)
    pred_df = pd.read_parquet(imputation_dir / "predicted-speeds.parquet")
    mono_df, mono_checks = check_monotonicity_and_peak_dip(pred_df)
    compass_df = run_compass_routing_smoke_test(open_net_dir, imputation_dir)

    print("\n=== 1. OUT-OF-SAMPLE ACCURACY (OVERALL) ===")
    print(overall_df.round(3).to_string(index=False))

    print("\n=== 1b. OUT-OF-SAMPLE ACCURACY BY ROAD CLASS ===")
    print(by_rc_df.round(3).to_string(index=False))

    print("\n=== 1c. OUT-OF-SAMPLE ACCURACY BY TIME BIN ===")
    print(by_bin_df.round(3).to_string(index=False))

    print("\n=== 2. MONOTONICITY & PEAK-DIP CHECK (IMPUTED SPEEDS MPH) ===")
    median_cols = [
        "road_class",
        "n_edges",
        "sl-med-imp",
        "wd-am-prd_median",
        "wd-af-prd_median",
        "wd-pm-prd_median",
        "we-am-prd_median",
        "we-af-prd_median",
        "we-pm-prd_median",
    ]
    print(mono_df[median_cols].round(2).to_string(index=False))
    failed_checks = [k for k, v in mono_checks.items() if not v]
    if failed_checks:
        raise AssertionError(f"Monotonicity / Peak-Dip checks failed: {failed_checks}")
    print(f"All {len(mono_checks)} monotonicity & peak-dip assertions PASSED.")

    print("\n=== 3. ROUTEE COMPASS ISOCHRONE SMOKE TEST (WD-AM / WD-PM vs WE-AF) ===")
    print(compass_df.to_string(index=False))
    assert (compass_df["wd-pm_destinations"] < compass_df["we-af_destinations"]).all(), (
        "Expected peak wd-pm reachable destinations to be strictly smaller than off-peak we-af"
    )
    assert (compass_df["wd-pm_area_km2"] < compass_df["we-af_area_km2"]).all(), (
        "Expected peak wd-pm isochrone area to contract relative to off-peak we-af"
    )
    assert (compass_df["wd-am_area_km2"] < compass_df["we-af_area_km2"]).all(), (
        "Expected peak wd-am isochrone area to contract relative to off-peak we-af"
    )
    print("RouteE Compass peak vs. off-peak isochrone contraction assertions PASSED.")

    if args.report_out is not None:
        args.report_out.parent.mkdir(parents=True, exist_ok=True)
        report_md = [
            "# BAMBAM Congestion Domain & Statistical Validation Report",
            "",
            "## 1. Out-of-Sample Accuracy (80/20 Panel Split by Link ID)",
            "### Overall",
            "```text",
            overall_df.round(3).to_string(index=False),
            "```",
            "",
            "### By Road Class",
            "```text",
            by_rc_df.round(3).to_string(index=False),
            "```",
            "",
            "### By Time Bin",
            "```text",
            by_bin_df.round(3).to_string(index=False),
            "```",
            "",
            "## 2. Monotonicity & Peak-Dip Check (Imputed Speeds by Road Class x Time Bin)",
            "```text",
            mono_df[median_cols].round(2).to_string(index=False),
            "```",
            "",
            "## 3. RouteE Compass Routing Smoke Test (Peak vs. Off-Peak Isochrones)",
            "```text",
            compass_df.to_string(index=False),
            "```",
            "",
        ]
        args.report_out.write_text("\n".join(report_md), encoding="utf-8")
        print(f"Wrote validation report to '{args.report_out}'")


if __name__ == "__main__":
    main()
