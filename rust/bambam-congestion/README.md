# `bambam-congestion`

High-performance Rust engine for **Congestion Calibration** (conflating OpenStreetMap / Overture Maps road networks with TomTom MultiNet-R speeds) and **Operating Speed Imputation** across the 6 BAMBAM / MEP time-of-day periods (`wd-am`, `wd-af`, `wd-pm`, `we-am`, `we-af`, `we-pm`).

## TomTom Data

This software will impute speeds into an OSM/OMF network dataset, trained from real speed data collected by TomTom. The TomTom network data itself is not licensed for open-source and additionally requires a processing step via an internal-NLR software package that produces RouteE Compass datasets. While this means that this code cannot be run outside of NLR, 1) it is provided here for review, 2) we may eventually publish speed datasets that result from this ML pipeline, as that does not conflict with our license.

---

## End-to-End Pipeline

1. **Import Open Network (`bambam_util import-network` / `bambam-osm` / `bambam-omf`):**
   Produces `<network_dir>/drive/` containing `edges-compass.csv.gz`, `edges-geometries-enumerated.txt.gz`, `edges-classes-enumerated.txt.gz`, `edges-posted-speeds-mph-enumerated.txt.gz`, and `vertices-complete.csv.gz`.
2. **Calibrate (`bambam-congestion calibrate` or `bambam_util congestion-calibrate`):**
   Streams the TomTom MultiNet-R Compass dataset alongside its 6 enumerated period speed files (`tomtom-natl-{wd,we}-{am,af,pm}.txt.gz`), conflates high-functional class edges (`Motorway`, `Trunk`, `Link`, `Primary`, `Secondary`, `Tertiary`) within a `5m` corridor and $\le 45^\circ$ circular heading difference, computes `0.5-mile` spatial densities (`Opp Density`, `Traffic Signal/Crossing/Traffic Stop Density`, `Population Density`, `Employment Density`), and writes:
   - `calibration_with_speedlimit.{parquet,csv}` (16 predictors + `Speed`)
   - `calibration_without_speedlimit.{parquet,csv}` (15 predictors + `Speed`)
   - `conflated_edges.{parquet,csv}` (wide per-edge summary)
3. **Train XGBoost Models (`python/nlr/bambam/congestion/train_congestion_models.py`):**
   Fits `XGBRegressor(tree_method="hist")` on the Parquet training tables and exports portable `xgboost_model_with_speedlimit.json` and `xgboost_model_without_speedlimit.json`.
4. **Impute (`bambam-congestion impute` or `bambam_util congestion-impute`):**
   Evaluates the calibrated `.json` models in parallel (`rayon`) on high-functional edges and assigns per-highway median speed limits (`sl-med-imp` $\ge 15.0\text{ mph}$) to low-functional edges (`Residential`, `Service`, `Other`), writing:
   - `edges-speeds-wd-am-mph-enumerated.txt.gz`
   - `edges-speeds-wd-af-mph-enumerated.txt.gz`
   - `edges-speeds-wd-pm-mph-enumerated.txt.gz`
   - `edges-speeds-we-am-mph-enumerated.txt.gz`
   - `edges-speeds-we-af-mph-enumerated.txt.gz`
   - `edges-speeds-we-pm-mph-enumerated.txt.gz`
   - `predicted-speeds.{parquet,csv}`

---

## Feature Schema & Ordering

| Index (`with_sl`) | Index (`without_sl`) | Feature Name | Unit / Definition |
| :---: | :---: | :--- | :--- |
| `0` | *(omitted)* | `Speed Limit` | Posted speed limit in `mph` (`None` if missing or `Road Class: Link`). |
| `1` | `0` | `Population Density` | `1,000 persons / sq mi` within `0.5 mi` buffer. |
| `2` | `1` | `Employment Density` | `1,000 jobs / sq mi` within `0.5 mi` buffer. |
| `3` | `2` | `Day: Weekday` | `1.0` for `wd-*`, `0.0` for `we-*`. |
| `4` | `3` | `Hour: AM Peak` | `1.0` for `*-am`, `0.0` otherwise. |
| `5` | `4` | `Hour: PM Peak` | `1.0` for `*-pm`, `0.0` otherwise. |
| `6..=13` | `5..=12` | `Road Class: *` | One-hot flags: `[Motorway, Trunk, Primary, Secondary, Tertiary, Service, Link, Other]`. |
| `14` | `13` | `Opp Density` | MEP opportunities per `sq mi` within `0.5 mi` buffer ($\pi R^2 + 2RL$). |
| `15` | `14` | `Traffic Signal/Crossing/Traffic Stop Density` | Traffic controls per `sq mi` within `0.5 mi` buffer ($\pi R^2 + 2RL$). |

See `configuration/bambam-congestion/congestion.toml` (`bambam_congestion::CongestionPipelineConfig`) for the unified pipeline configuration example.

## Example

Assumes the following:
  - all of the time-binned speed files (`[[time_bins]]`) and their reference speed dataset resolution (`speeds_dir` + `file_prefix`/`file_pattern`, or explicit per-bin filenames) are configured in `configuration/bambam-congestion/congestion.toml`.
  - those speeds files are flat (Compass "enumerated"-style) files matching row-by-row with a reference road network dataset in RouteE Compass format.

```sh
# 0. Build the release binaries from the repository root
cargo build --manifest-path rust/Cargo.toml --release -p bambam-congestion -p bambam

# 1. (Optional) Import an OpenStreetMap or Overture Maps (OMF) road network into RouteE Compass format
RUST_LOG=info ./rust/target/release/bambam_util import-network \
    --source omf \
    --extent-file query/boulder_extent.json \
    --output-directory output/boulder_network

# (Optional) Inspect the configured time bins programmatically
./rust/target/release/bambam-congestion list-time-bins \
    --config configuration/bambam-congestion/congestion.toml

# 2. Conflate the Open Network with the Reference Network speeds and compute 0.5-mile spatial features
#    Produces calibration_with_speedlimit.parquet and calibration_without_speedlimit.parquet in [artifacts].calibration_dir
RUST_LOG=info ./rust/target/release/bambam-congestion calibrate \
    --config configuration/bambam-congestion/congestion.toml

# 3. Train the calibrated XGBoost models (`with_speedlimit` and `without_speedlimit`)
#    Reads [artifacts].calibration_dir and exports portable .json models to [artifacts].models_dir
uv run python python/nlr/bambam/congestion/train_congestion_models.py \
    --config configuration/bambam-congestion/congestion.toml \
    --n-iter 25

# 4. Impute operating speeds across all configured time bins
#    Reads models from [artifacts].models_dir and writes edges-speeds-{bin}-mph-enumerated.txt.gz
#    and predicted-speeds.parquet into output/boulder_network/drive/
RUST_LOG=info ./rust/target/release/bambam-congestion impute \
    --config configuration/bambam-congestion/congestion.toml
```

## Validation

Validation runs executed by Gemini Argon, 2026-11-10:
  - [Conflation](#conflation)
    - **What was tested:** End-to-end calibration (`bambam-congestion calibrate`), `0.5-mile` spatial feature extraction (`Opp Density`, `Traffic Control Density`, `Population/Employment Density`), XGBoost model training (`train_congestion_models.py`), and Rust speed imputation (`bambam-congestion impute`) on the `boulder_co` road network (`7,885` edges) against the national TomTom MultiNet-R reference dataset (`124.4M` edges, `33.5M` time-varying profiles). Also verified numerical parity between Python `xgboost.XGBRegressor` and Rust `XgboostJsonModel` in `python/tests/test_congestion_pipeline.py`.
    - **What was found:** Conflated `98.22%` (`2,097 / 2,135`) of high-functional edges in `205.9s` (`5.27 GB` peak RSS), confirmed directional speed separation on divided highways (`US-36` morning inbound slowdown vs. evening outbound slowdown) with zero opposing-carriageway false matches, verified `0` `NaN`s across all spatial features and imputed speed tables, and confirmed `< 1e-5 mph` numerical parity between Python and Rust XGBoost evaluators.
  - [Smoke Test](#routee-compassbambamrunner-routing-smoke-test)
    - **What was tested:** Statistical and domain validation (`validate_congestion_results.py`) covering out-of-sample accuracy (`80/20` panel split by link ID), functional-class monotonicity and peak-dip invariants across all `6` time bins, and live RouteE Compass (`BambamRunner`) Dijkstra drive-mode isochrone searches (`2`, `4`, and `6` minute horizons across `3` Boulder origins) comparing peak (`wd-am`, `wd-pm`) vs. off-peak (`we-af`) imputed speed tables.
    - **What was found:** Out-of-sample combined test accuracy achieved `RMSE = 6.30 mph` (`MAE = 4.76 mph`, `SMAPE = 23.83%`); all `18` functional-class hierarchy (`Motorway > Trunk > Primary > Secondary > Tertiary > Residential`) and peak-dip (`wd-pm < we-af`, `wd-am < we-am`) assertions passed; and RouteE Compass peak-hour (`wd-pm`) drive isochrones contracted by `13.4%–34.6%` in area (and `38–429` fewer reachable destinations) relative to off-peak (`we-af`).

### Conflation

Conflation was tested on the [boulder_co/](/boulder_co/) scenario.

#### `bambam-congestion calibrate` Benchmarks & Conflation Validation
- **Performance & Memory:**
  - **Wall-clock time:** `205.9s` (`3.4 min` total: `12s` for spatial feature R*-trees, `25s` to load `33,512,241` time-varying speed profiles, `168s` to stream `124,379,003` TomTom reference edges + `6` gzipped period speed files, `< 0.5s` for parallel `rayon` conflation and Parquet/CSV export).
  - **Peak Memory (RSS):** `5.27 GB` (`4.99 GB` peak footprint), well below the `16 GB` ceiling.
- **Match Rate by Road Class (`2,097 / 2,135 = 98.22%` overall):**

| Road Class | Open Network Edges | Conflated Edges | Match Rate (%) | Median `wd-am` (mph) | Median `wd-af` (mph) | Median `wd-pm` (mph) | Median `we-af` (mph) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `motorway` | 16 | 16 | **100.00%** | 41.84 | 45.91 | 44.41 | 47.63 |
| `primary` | 332 | 329 | **99.10%** | 26.65 | 26.34 | 23.88 | 27.49 |
| `tertiary` | 1,083 | 1,073 | **99.08%** | 20.82 | 21.06 | 20.62 | 21.28 |
| `link` | 84 | 82 | **97.62%** | 20.34 | 20.59 | 18.91 | 21.73 |
| `trunk` | 131 | 127 | **96.95%** | 31.13 | 32.13 | 28.97 | 33.58 |
| `secondary` | 489 | 470 | **96.11%** | 26.50 | 26.03 | 24.14 | 27.45 |

- **Divided Highway Directional Check (US-36 Motorway):**
  - **WB/NB into Boulder (`8` edges):** Heavy morning inbound congestion (`wd-am` mean `32.43 mph` vs. `wd-pm` mean `36.87 mph`, $\Delta_{\text{AM}-\text{PM}} = -4.44\text{ mph}$).
  - **EB/SB towards Denver (`8` edges):** Free-flowing morning outbound (`wd-am` mean `48.62 mph`) vs. slower evening outbound (`wd-pm` mean `42.17 mph`, $\Delta_{\text{AM}-\text{PM}} = +6.45\text{ mph}$).
  - Confirms the $\le 45^\circ$ geodesic circular heading filter cleanly separates opposing carriageways.

---

#### 3. Step 2.2 — Spatial Feature Distributions
- **Sources:** `colorado-latest.osm.pbf` (`3,203` opportunity points), `boulder_co/vertices-complete.csv.gz` (`171` traffic control vertices), and `tracts.csv` + `2025-12-09-aceee-poverty-population.csv` + `LODES8_wac_2022_JT00_S000_census_tract.csv.gz` (`39` Census tracts in the Boulder study bbox).
- **Summary Statistics (`calibration_with_speedlimit.parquet`, `11,592` rows, `0` `NaN`s):**

| Feature | Min | Median | Mean | Max | `% Non-Zero` | `NaN` Count |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `Speed Limit` (mph) | 10.00 | 30.00 | 29.57 | 60.00 | 100.0% | 0 |
| `Population Density` ($10^3 / \text{mi}^2$) | 0.026 | 1.477 | 2.231 | 12.275 | 100.0% | 0 |
| `Employment Density` ($10^3 / \text{mi}^2$) | 0.028 | 3.617 | 5.677 | 18.471 | 100.0% | 0 |
| `Opp Density` ($\text{opps} / \text{mi}^2$) | 6.69 | 184.29 | 247.39 | 665.88 | 100.0% | 0 |
| `Traffic Signal/Crossing/Stop Density` ($\text{controls} / \text{mi}^2$) | 0.00 | 12.46 | 13.06 | 39.89 | 93.5% | 0 |
| `Speed` (mph) | 2.53 | 24.24 | 23.80 | 66.44 | 100.0% | 0 |

---

#### 4. Step 2.3 — Model Training & `bambam-congestion impute`
- **Model Training (`train_congestion_models.py` with `--n-iter 25`):**
  - `xgboost_model_with_speedlimit`: $\text{RMSE} = 6.17\text{ mph}$, $\text{MAE} = 4.67\text{ mph}$, $\text{SMAPE} = 22.83\%$ (`5.61s`).
  - `xgboost_model_without_speedlimit`: $\text{RMSE} = 7.48\text{ mph}$, $\text{MAE} = 5.61\text{ mph}$, $\text{SMAPE} = 34.16\%$ (`1.15s`).
- **Imputation (`bambam-congestion impute`):**
  - Completed in `12.09s` (`98.6 MB` peak RSS) for all `7,885` edges (`2,135` high-functional via XGBoost + `5,750` low-functional via `sl-med-imp`).
  - Verified that all 6 `edges-speeds-{wd,we}-{am,af,pm}-mph-enumerated.txt.gz` files have exactly `7,885` rows matching `boulder_co/edges-compass.csv.gz` with zero `NaN`, negative, or zero speeds.

### RouteE Compass/BamBamRunner Routing Smoke Test

Created `validate_congestion_results.py` and generated `validation_report.md`:

#### 1. Out-of-Sample Accuracy (80/20 Panel Split by Link ID)
- **Overall (`n = 2,376` held-out edge $\times$ period observations):**

| Model Group | Test Samples | Mean Observed (mph) | Mean Predicted (mph) | RMSE (mph) | MAE (mph) | SMAPE (%) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `with_speedlimit` | 2,166 | 23.36 | 22.62 | **6.17** | **4.68** | **22.83%** |
| `without_speedlimit` | 210 | 18.89 | 19.06 | **7.48** | **5.61** | **34.16%** |
| **Combined All Test** | **2,376** | **22.97** | **22.30** | **6.30** | **4.76** | **23.83%** |

- **By `CongestionRoadClass`:**

| Road Class | Test Samples | Mean Observed (mph) | Mean Predicted (mph) | RMSE (mph) | MAE (mph) | SMAPE (%) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `Motorway` | 24 | 53.71 | 43.26 | 12.19 | 10.50 | 23.37% |
| `Trunk` | 120 | 30.50 | 28.97 | 8.11 | 6.28 | 23.62% |
| `Primary` | 354 | 26.48 | 25.31 | 7.51 | 5.84 | 25.11% |
| `Secondary` | 522 | 25.28 | 24.03 | 6.95 | 5.58 | 26.61% |
| `Tertiary` | 1,230 | 19.89 | 20.00 | 4.97 | 3.71 | 20.97% |
| `Link` | 126 | 20.53 | 18.81 | 7.52 | 6.02 | 36.96% |

- **By `TimeBinSpec`:**

| Time Bin | Test Samples | Mean Observed (mph) | Mean Predicted (mph) | RMSE (mph) | MAE (mph) | SMAPE (%) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| `wd-am` | 396 | 22.42 | 21.87 | 6.23 | 4.69 | 23.83% |
| `wd-af` | 396 | 22.65 | 21.88 | 6.32 | 4.79 | 24.14% |
| `wd-pm` | 396 | 21.58 | 20.91 | 6.48 | 4.97 | 26.25% |
| `we-am` | 396 | 24.18 | 23.48 | 6.11 | 4.59 | 21.95% |
| `we-af` | 396 | 23.57 | 22.99 | 6.28 | 4.75 | 23.25% |
| `we-pm` | 396 | 23.41 | 22.69 | 6.33 | 4.75 | 23.58% |

---

#### 2. Monotonicity & Peak-Dip Check (Median Imputed Speeds in `mph` Across All `7,885` Edges)
All 18 programmatic assertions (`Motorway > Trunk > Primary > Secondary > Tertiary > Residential` across every time bin, and `wd-pm < we-af` / `wd-am < we-am` across all high-functional road classes) **passed**:

| Road Class | Edges | `sl-med-imp` (mph) | `wd-am` (mph) | `wd-af` (mph) | `wd-pm` (mph) | `we-am` (mph) | `we-af` (mph) | `we-pm` (mph) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| `Motorway` | 16 | 45.0 | **35.99** | 36.70 | **35.39** | 40.48 | 40.53 | 39.82 |
| `Trunk` | 131 | 40.0 | **30.63** | 31.40 | **28.20** | 33.39 | 32.55 | 31.95 |
| `Primary` | 332 | 35.0 | **26.15** | 25.65 | **23.54** | 28.57 | 27.31 | 26.59 |
| `Secondary` | 489 | 35.0 | **25.36** | 25.13 | **22.76** | 27.13 | 26.57 | 26.06 |
| `Tertiary` | 1,083 | 25.0 | **20.40** | 20.58 | **20.11** | 21.29 | 21.07 | 20.93 |
| `Link` | 84 | 40.0 | **20.17** | 20.10 | **18.81** | 22.20 | 21.90 | 21.44 |
| `Residential` | 5,380 | 20.0 | 20.00 | 20.00 | 20.00 | 20.00 | 20.00 | 20.00 |
| `Other` | 370 | 20.0 | 20.00 | 20.00 | 20.00 | 20.00 | 20.00 | 20.00 |

---

#### 3. RouteE Compass (`BambamRunner`) Routing Smoke Test
Executed Dijkstra drive-mode searches across 3 Boulder origins (`center`, `southwest_core`, `northeast_core`) at `2`, `4`, and `6` minute time bins using `edges-speeds-wd-am-mph-enumerated.txt.gz`, `edges-speeds-wd-pm-mph-enumerated.txt.gz`, and `edges-speeds-we-af-mph-enumerated.txt.gz`:
- Across all origins and time thresholds, peak-hour `wd-pm` isochrone area contracts by **`13.4%` to `34.6%`** (and reachable network vertices drop by **`38` to `429` destinations**) relative to off-peak `we-af`.
- Morning peak `wd-am` isochrone area also contracts across every origin and threshold relative to `we-af`.
