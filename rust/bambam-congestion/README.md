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