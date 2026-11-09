# `bambam-congestion`

High-performance Rust engine for **Congestion Calibration** (conflating OpenStreetMap / Overture Maps road networks with TomTom MultiNet-R speeds) and **Operating Speed Imputation** across the 6 BAMBAM / MEP time-of-day periods (`wd-am`, `wd-af`, `wd-pm`, `we-am`, `we-af`, `we-pm`).

---

## Subcommands

### 1. `calibrate` (`bambam-congestion calibrate` or `bambam_util congestion-calibrate`)

Conflates a BAMBAM / RouteE Compass Open Network (`bambam-osm` or `bambam-omf`) with a TomTom MultiNet-R RouteE Compass dataset, computes 0.5-mile spatial features (`Opp Density`, `Traffic Signal/Crossing/Traffic Stop Density`, `Population Density`, `Employment Density`), and writes calibration training datasets (`calibration_with_speedlimit.{parquet,csv}`, `calibration_without_speedlimit.{parquet,csv}`, and `conflated_edges.{parquet,csv}`).

#### Example `calibration_config.toml`
```toml
[open_network]
network_dir = "boulder_co"
mode = "drive"
legacy_posted_speed_unit = "kph"
min_imputed_speed_limit_mph = 15.0

[tomtom_network]
network_dir = "/Users/rfitzger/dev/nrel/routee/routee-compass-tomtom/data/tomtom_national"
speed_unit = "kph"
require_time_varying_trace = true

[tomtom_network.period_speed_files]
wd_am = "/Users/rfitzger/data/bam/mep3/in/congestion/tomtom-natl-wd-am.txt.gz"
wd_af = "/Users/rfitzger/data/bam/mep3/in/congestion/tomtom-natl-wd-af.txt.gz"
wd_pm = "/Users/rfitzger/data/bam/mep3/in/congestion/tomtom-natl-wd-pm.txt.gz"
we_am = "/Users/rfitzger/data/bam/mep3/in/congestion/tomtom-natl-we-am.txt.gz"
we_af = "/Users/rfitzger/data/bam/mep3/in/congestion/tomtom-natl-we-af.txt.gz"
we_pm = "/Users/rfitzger/data/bam/mep3/in/congestion/tomtom-natl-we-pm.txt.gz"

[conflation]
buffer_distance_meters = 5.0
max_heading_diff_degrees = 45.0
min_overlap_meters = 1.0
cap_speed_outliers = true

[spatial_features]
density_radius_miles = 0.5
projection_mode = "geodesic"

[output]
output_dir = "output/congestion_calibration"
write_parquet = true
write_csv = true
```

### 2. `impute` (`bambam-congestion impute` or `bambam_util congestion-impute`)

Applies calibrated `with_speedlimit` and `without_speedlimit` XGBoost JSON models to high-functional class edges (`Motorway`, `Trunk`, `Link`, `Primary`, `Secondary`, `Tertiary`) and per-class median posted speeds (`sl-med-imp` $\ge 15\text{ mph}$) to low-functional class edges (`Residential`, `Service`, `Other`), writing RouteE Compass enumerated speed files (`edges-speeds-{period}-mph-enumerated.txt.gz`) and `predicted-speeds.{parquet,csv}`.
