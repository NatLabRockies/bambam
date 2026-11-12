#!/bin/bash

# requires CENSUS_API_KEY (or CENSUS_API_TOKEN) set in environment

# # please compile the rust and python code:
# source /home/rfitzger/dev/bambam/.venv/bin/activate
# compile bambam (rust/py)
# cargo build --release --manifest-path rust/Cargo.toml
# uv sync
# uv pip install ".[all]"

set -euo pipefail

# Defaults (can be overridden via CLI flags or environment variables)
REPO_ROOT="$(pwd)"
NETWORK_CONFIG="${NETWORK_CONFIG:-configuration/bambam-network/bambam-network-config.json}"
CONGESTION_CONFIG="${CONGESTION_CONFIG:-configuration/bambam-congestion/congestion.toml}"
EXTENT_FILE="${EXTENT_FILE:-}"
OUTPUT_DIR="${OUTPUT_DIR:-}"
GTFS_MANIFEST="${GTFS_MANIFEST:-}"
GTFS_SUBDIVISION="${GTFS_SUBDIVISION:-}"
REF_NETWORK_DIR="${REF_NETWORK_DIR:-}"
REF_SPEEDS_DIR="${REF_SPEEDS_DIR:-}"

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Required arguments:
  -e, --extent-file <FILE>        WKT extent file (e.g., us-all-wkt.txt)
  -o, --output-dir <DIR>          Output network directory (e.g., us_network)
  -m, --gtfs-manifest <FILE>      Mobility Data Catalog CSV file for GTFS download
  -r, --ref-network-dir <DIR>     Reference road network directory (e.g., TomTom Compass dataset)

Optional arguments:
  -s, --ref-speeds-dir <DIR>      Directory with reference time-bin speed files (defaults to --ref-network-dir)
  -d, --subdivision-name <NAME>   Optional state/subdivision filter for GTFS download (e.g., "Colorado")
  -n, --network-config <FILE>     Network import config (default: ${NETWORK_CONFIG})
  -c, --congestion-config <FILE>  Congestion pipeline config (default: ${CONGESTION_CONFIG})
  -h, --help                      Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -e|--extent-file)
            EXTENT_FILE="$2"
            shift 2
            ;;
        -o|--output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -m|--gtfs-manifest|--transit-manifest)
            GTFS_MANIFEST="$2"
            shift 2
            ;;
        -d|--subdivision-name|--gtfs-subdivision)
            GTFS_SUBDIVISION="$2"
            shift 2
            ;;
        -r|--ref-network-dir|--reference-network-dir)
            REF_NETWORK_DIR="$2"
            shift 2
            ;;
        -s|--ref-speeds-dir|--reference-speeds-dir)
            REF_SPEEDS_DIR="$2"
            shift 2
            ;;
        -n|--network-config)
            NETWORK_CONFIG="$2"
            shift 2
            ;;
        -c|--congestion-config)
            CONGESTION_CONFIG="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Error: Unknown argument '$1'" >&2
            usage >&2
            exit 1
            ;;
    esac
done

# Default REF_SPEEDS_DIR to REF_NETWORK_DIR if not separately provided
REF_SPEEDS_DIR="${REF_SPEEDS_DIR:-${REF_NETWORK_DIR}}"

# Validate required arguments
MISSING=0
for var_name in EXTENT_FILE OUTPUT_DIR GTFS_MANIFEST REF_NETWORK_DIR; do
    if [[ -z "${!var_name}" ]]; then
        echo "Error: Missing required argument ${var_name}" >&2
        MISSING=1
    fi
done
if [[ "${MISSING}" -eq 1 ]]; then
    echo "" >&2
    usage >&2
    exit 1
fi

# Resolve GTFS_MANIFEST to an absolute path so subshells that `cd` can read it
GTFS_MANIFEST="$(cd "$(dirname "${GTFS_MANIFEST}")" && pwd)/$(basename "${GTFS_MANIFEST}")"

# Derived paths inside OUTPUT_DIR
GTFS_RAW_DIR="${OUTPUT_DIR}/gtfs"
TRANSIT_DIR="${OUTPUT_DIR}/transit"
CENSUS_CSV="${OUTPUT_DIR}/census_tracts_bamcensus.csv.gz"
CALIBRATION_DIR="${OUTPUT_DIR}/congestion_calibration"
MODELS_DIR="${OUTPUT_DIR}/congestion_models"

# download OvertureMaps network dataset
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-omf" network \
    --configuration-file "${NETWORK_CONFIG}" \
    --extent-file "${EXTENT_FILE}" \
    --output-directory "${OUTPUT_DIR}"

# download GTFS dataset
mkdir -p "${GTFS_RAW_DIR}"
DOWNLOAD_ARGS=(
    --manifest-file "${GTFS_MANIFEST}"
    --country-code US
    --data-type gtfs
    --parallelism 16
)
if [[ -n "${GTFS_SUBDIVISION}" ]]; then
    DOWNLOAD_ARGS+=(--subdivision-name "${GTFS_SUBDIVISION}")
fi
(
    cd "${GTFS_RAW_DIR}" && \
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-transit" download "${DOWNLOAD_ARGS[@]}"
)

# import GTFS archives into vectorized routing table
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-transit" import \
    --input "${GTFS_RAW_DIR}" \
    --parallelism 32 \
    --output-directory "${TRANSIT_DIR}" \
    --vertices-compass-filename "${OUTPUT_DIR}/vertices-compass.csv.gz" \
    --extent-file "${EXTENT_FILE}" \
    --start-date 09-01-2025 \
    --end-date 09-01-2025 \
    --start-time 08:00:00 \
    --end-time 09:00:00 \
    --date-mapping-policy nearest-date-time-range \
    --date-mapping-date-tolerance 730 \
    --date-mapping-match-weekday true \
    --missing-stop-location-policy drop

# inject transit mode into config file
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam_util" gtfs-config \
    --directory "${TRANSIT_DIR}"/transit \
    --base-config "${OUTPUT_DIR}/bambam.toml"

# collect census geometries, population, employment for congestion imputation
# requires CENSUS_API_KEY
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" export-census \
    --open-network-dir "${OUTPUT_DIR}" \
    --output-csv "${CENSUS_CSV}"

# calibrate reference network to routing network
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" calibrate \
    --config "${CONGESTION_CONFIG}" \
    --open-network-dir "${OUTPUT_DIR}" \
    --mode drive \
    --census-csv "${CENSUS_CSV}" \
    --reference-network-dir "${REF_NETWORK_DIR}" \
    --reference-speeds-dir "${REF_SPEEDS_DIR}" \
    --output-dir "${CALIBRATION_DIR}"

# train xgboost model on reference network
uv run python python/nlr/bambam/congestion/train_congestion_models.py \
    --calibration-dir "${CALIBRATION_DIR}" \
    --output-dir "${MODELS_DIR}"

# impute speeds from xgboost model into routing network representation
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" impute \
    --config "${CONGESTION_CONFIG}" \
    --open-network-dir "${OUTPUT_DIR}" \
    --mode drive \
    --census-csv "${CENSUS_CSV}" \
    --models-dir "${MODELS_DIR}" \
    --output-dir "${OUTPUT_DIR}/drive"

# calculate WCI for walk mode
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam_util" modal_metric \
    --modal-metric WCI \
    --network-dir "${OUTPUT_DIR}" \
    --output-file "${OUTPUT_DIR}/walk/wci.csv.gz"

# calculate LTS for bike mode
RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam_util" modal_metric \
    --modal-metric LTS \
    --network-dir "${OUTPUT_DIR}" \
    --output-file "${OUTPUT_DIR}/bike/lts.csv.gz"

echo "finished."