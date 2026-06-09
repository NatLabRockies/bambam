#!/bin/sh

# 0. turn on logging, blow up script if fails
set -e
export RUST_LOG="info"
WORKING_DIR="out/minnesota-flex"

# 1. compile bambam
cargo build -r --manifest-path rust/Cargo.toml

# 2. download OMF network covering test flex dataset region, SW of Saint Paul, MN
#    create networks for walk, bike, and drive-mode travel (using posted speeds for drive)
rust/target/release/bambam-omf network -n minnesota-flex -c configuration/bambam-omf/travel-mode-filter.json -o "$WORKING_DIR" -b -95.14435,-93.76282,44.01652,44.568947

# 3. process GTFS-Flex datasets
rust/target/release/bambam-gtfs-flex import rust/bambam-gtfs-flex/src/test/assets/flex "$WORKING_DIR" 20240903

# 4. inject the GTFS-Flex 
rust/target/release/bambam_util gtfs-flex-config-app --base-file "$WORKING_DIR"/bambam.toml --out-file "$WORKING_DIR"/bambam-gtfs-flex.toml --flex-directory "$WORKING_DIR" --start-time '2024-09-03T06:00:00' --graph-edge-list 2 --map-edge-list 2 --gtfs-flex-edge-list 2

# 5. prepare the grid region
echo '{ "extent": "POLYGON((-95.14435 44.01652, -95.14435 44.568947, -93.76282 44.568947, -93.76282 44.01652, -95.14435 44.01652))" }' > "$WORKING_DIR"/query.json

# 6. run BAMBAM
rust/target/release/bambam -c "$WORKING_DIR"/bambam-gtfs-flex.toml -q "$WORKING_DIR"/query.json