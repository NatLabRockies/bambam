"""
Train calibrated XGBoost operating speed prediction models (`with_speedlimit` and
`without_speedlimit`) from Parquet/CSV tables produced by `bambam-congestion calibrate`,
and export portable `.json` (for Rust `bambam-congestion impute`) and `.joblib` models.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import time
import tomllib
from typing import Sequence

from joblib import dump
import numpy as np
import pandas as pd
from sklearn.metrics import mean_absolute_error, mean_squared_error
from sklearn.model_selection import RandomizedSearchCV, train_test_split
from scipy.stats import randint, uniform
import xgboost as xgb

PREDICTORS_WITH_SPEEDLIMIT: list[str] = [
    "Speed Limit",
    "Population Density",
    "Employment Density",
    "Day: Weekday",
    "Hour: AM Peak",
    "Hour: PM Peak",
    "Road Class: Motorway",
    "Road Class: Trunk",
    "Road Class: Primary",
    "Road Class: Secondary",
    "Road Class: Tertiary",
    "Road Class: Service",
    "Road Class: Link",
    "Road Class: Other",
    "Opp Density",
    "Traffic Signal/Crossing/Traffic Stop Density",
]

PREDICTORS_WITHOUT_SPEEDLIMIT: list[str] = [
    "Population Density",
    "Employment Density",
    "Day: Weekday",
    "Hour: AM Peak",
    "Hour: PM Peak",
    "Road Class: Motorway",
    "Road Class: Trunk",
    "Road Class: Primary",
    "Road Class: Secondary",
    "Road Class: Tertiary",
    "Road Class: Service",
    "Road Class: Link",
    "Road Class: Other",
    "Opp Density",
    "Traffic Signal/Crossing/Traffic Stop Density",
]


def load_table(path: Path) -> pd.DataFrame:
    if path.suffix.lower() == ".parquet":
        return pd.read_parquet(path)
    return pd.read_csv(path)


def fit_and_export_model(
    df: pd.DataFrame,
    predictors: Sequence[str],
    output_dir: Path,
    model_stem: str,
    n_iter: int = 25,
    cv: int = 5,
    max_search_rows: int = 2_000_000,
    random_state: int = 42,
) -> dict[str, float]:
    output_dir.mkdir(parents=True, exist_ok=True)
    df = df.dropna(subset=["Speed"]).reset_index(drop=True)

    # Panel split by source ID so all 6 time periods of a link stay in the same split
    id_col = "ID" if "ID" in df.columns else "edge_id"
    unique_ids = df[id_col].unique()
    train_ids, _ = train_test_split(unique_ids, train_size=0.8, random_state=random_state)
    train_mask = df[id_col].isin(train_ids)
    train_df = df[train_mask]
    test_df = df[~train_mask]

    X_train = train_df[list(predictors)].astype(np.float32)
    y_train = train_df["Speed"].astype(np.float32)
    X_test = test_df[list(predictors)].astype(np.float32)
    y_test = test_df["Speed"].astype(np.float32)

    start_time = time.time()
    if n_iter > 1 and len(X_train) >= cv * 2:
        # Subsample for hyperparameter search if national dataset exceeds max_search_rows
        if len(X_train) > max_search_rows:
            rng = np.random.default_rng(random_state)
            sub_idx = rng.choice(len(X_train), size=max_search_rows, replace=False)
            X_search = X_train.iloc[sub_idx]
            y_search = y_train.iloc[sub_idx]
        else:
            X_search, y_search = X_train, y_train

        param_grid = {
            "n_estimators": randint(100, 401),
            "max_depth": randint(4, 15),
            "learning_rate": uniform(0.01, 0.29),
            "subsample": uniform(0.7, 0.3),
            "colsample_bytree": uniform(0.7, 0.3),
            "colsample_bylevel": uniform(0.7, 0.3),
            "gamma": uniform(0, 5),
            "reg_alpha": uniform(0, 1),
            "reg_lambda": uniform(0, 1),
            "min_child_weight": randint(1, 10),
        }
        search = RandomizedSearchCV(
            estimator=xgb.XGBRegressor(tree_method="hist", random_state=random_state),
            param_distributions=param_grid,
            n_iter=n_iter,
            cv=cv,
            n_jobs=-1,
            scoring="neg_mean_squared_error",
            random_state=random_state,
        )
        search.fit(X_search, y_search)
        best_params = search.best_params_
        best_model = xgb.XGBRegressor(
            tree_method="hist", random_state=random_state, **best_params
        )
        best_model.fit(X_train, y_train)
    else:
        best_model = xgb.XGBRegressor(
            n_estimators=200,
            max_depth=8,
            learning_rate=0.08,
            tree_method="hist",
            random_state=random_state,
        )
        best_model.fit(X_train, y_train)

    train_secs = time.time() - start_time

    # Save both portable JSON (for Rust bambam-congestion) and legacy .joblib
    json_path = output_dir / f"{model_stem}.json"
    joblib_path = output_dir / f"{model_stem}.joblib"
    best_model.get_booster().save_model(str(json_path))
    dump(best_model, joblib_path)

    preds = best_model.predict(X_test)
    rmse = float(np.sqrt(mean_squared_error(y_test, preds)))
    mae = float(mean_absolute_error(y_test, preds))
    denom = np.abs(y_test) + np.abs(preds)
    smape = float(
        np.mean(np.where(denom > 0, 2.0 * np.abs(y_test - preds) / denom * 100.0, 0.0))
    )

    metrics_path = output_dir / f"{model_stem}_metrics.txt"
    metrics_path.write_text(
        f"Model: {model_stem}\n"
        f"Train rows: {len(X_train)}\n"
        f"Test rows: {len(X_test)}\n"
        f"Training time (s): {train_secs:.2f}\n"
        f"RMSE (mph): {rmse:.4f}\n"
        f"MAE (mph): {mae:.4f}\n"
        f"SMAPE (%): {smape:.4f}\n"
    )

    return {"rmse": rmse, "mae": mae, "smape": smape, "train_secs": train_secs}


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Train calibrated XGBoost congestion speed models from bambam-congestion calibrate output"
    )
    parser.add_argument(
        "-c",
        "--config",
        type=Path,
        default=None,
        help="Optional path to unified congestion.toml (reads [artifacts].calibration_dir and [artifacts].models_dir)",
    )
    parser.add_argument(
        "--calibration-dir",
        type=Path,
        default=None,
        help="Override: directory containing calibration_with_speedlimit.parquet (or .csv) and calibration_without_speedlimit.parquet (or .csv)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help="Override: directory to write trained .json and .joblib models and validation metrics",
    )
    parser.add_argument(
        "--n-iter",
        type=int,
        default=25,
        help="Number of RandomizedSearchCV iterations (set to 1 for fast single-fit training)",
    )
    args = parser.parse_args()

    cfg_calib_dir: Path | None = None
    cfg_models_dir: Path | None = None
    if args.config is not None:
        with args.config.open("rb") as f:
            toml_data = tomllib.load(f)
        artifacts = toml_data.get("artifacts", {})
        if "calibration_dir" in artifacts:
            cfg_calib_dir = Path(artifacts["calibration_dir"])
        if "models_dir" in artifacts:
            cfg_models_dir = Path(artifacts["models_dir"])

    calibration_dir = args.calibration_dir or cfg_calib_dir
    output_dir = args.output_dir or cfg_models_dir
    if calibration_dir is None or output_dir is None:
        parser.error("must provide either --config congestion.toml or both --calibration-dir and --output-dir")

    with_sl_file = (
        calibration_dir / "calibration_with_speedlimit.parquet"
        if (calibration_dir / "calibration_with_speedlimit.parquet").exists()
        else calibration_dir / "calibration_with_speedlimit.csv"
    )
    without_sl_file = (
        calibration_dir / "calibration_without_speedlimit.parquet"
        if (calibration_dir / "calibration_without_speedlimit.parquet").exists()
        else calibration_dir / "calibration_without_speedlimit.csv"
    )

    if with_sl_file.exists():
        df_with = load_table(with_sl_file)
        m_with = fit_and_export_model(
            df_with,
            PREDICTORS_WITH_SPEEDLIMIT,
            output_dir,
            "xgboost_model_with_speedlimit",
            n_iter=args.n_iter,
        )
        print(f"Trained with_speedlimit model: {m_with}")

    if without_sl_file.exists():
        df_without = load_table(without_sl_file)
        m_without = fit_and_export_model(
            df_without,
            PREDICTORS_WITHOUT_SPEEDLIMIT,
            output_dir,
            "xgboost_model_without_speedlimit",
            n_iter=args.n_iter,
        )
        print(f"Trained without_speedlimit model: {m_without}")


if __name__ == "__main__":
    main()
