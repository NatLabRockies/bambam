#!/bin/sh

set -e

# This script sets up the developer environment for testing multimodal routing functionlity of bambam. this is
# intended to be run once after cloning the repository and building with -r flag.

# 0. Confirm Rust binaries are built
echo "build BAMBAM (if needed)"
cargo build -r --manifest-path rust/Cargo.toml

# 1. Download GTFS archive
echo "Download the Denver RTD GTFS archive to denver_rtd/rtd-gtfs.zip"
mkdir denver_rtd
curl -L -o denver_rtd/rtd_gtfs.zip https://www.rtd-denver.com/files/gtfs/google_transit.zip
echo "Unzip the archive"
unzip denver_rtd/rtd_gtfs.zip -d "denver_rtd/gtfs/"

# 2. Using RouteE Compass, download a graph of the Denver Metro region _with output geometries_
echo "Prepare the compass files"
uv run --with "geopandas,numpy,osmnx,nlr.routee.compass[all]" script/setup_test_bambam_gtfs.py denver_rtd --output_geometries

# 3. Create RouteE Compass edge list inputs for this archive
echo "Process gtfs archive for date matching"
rust/target/release/bambam_gtfs preprocess-bundle \
    --input "denver_rtd/rtd_gtfs.zip" \
    --starting-edge-list-id 1 \
    --parallelism 1 \
    --vertex-match-tolerance 2500 \
    --date-mapping-policy nearest-date-time-range \
    --date-mapping-date-tolerance 365 \
    --date-mapping-match-weekday true \
    --output-directory "denver_rtd/transit" \
    --vertices-compass-filename "denver_rtd/compass/vertices-compass.csv.gz" \
    --start-date 09-01-2025 \
    --end-date 09-01-2025 \
    --start-time 08:00:00 \
    --end-time 09:00:00

# 4. generate transit geometries to assist visualization
echo "Produce transit geometries"
uv run --with "geopandas,numpy" script/transit_output_extract_geojson.py --suffix="-transit-1" denver_rtd/transit/edges-compass-1.csv.gz denver_rtd/compass/vertices-compass.csv.gz denver_rtd/geometries

# 5. update a base configuration with the transit edge lists
echo "running gtfs-config to modify an existing BAMBAM TOML configuration with this transit dataset"
rust/target/release/bambam_util gtfs-config --directory denver_rtd/transit --base-config configuration/test_gtfs_config_denver_rtd.toml
mv configuration/test_gtfs_config_denver_rtd.toml denver_rtd/compass/test_gtfs_config_denver_rtd.toml

# echo "running BAMBAM with a walk-transit trip"
# ./rust/target/release/bambam -c configuration/test_gtfs_config_denver_rtd_gtfs.toml -q denver_rtd/geometries/query.json  