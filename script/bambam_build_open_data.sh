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
FROM_STAGE="${FROM_STAGE:-}"
AFTER_STAGE="${AFTER_STAGE:-}"
FORCE_RERUN=0

STAGES=(
    "network"
    "gtfs-download"
    "gtfs-import"
    "gtfs-config"
    "census"
    "calibrate"
    "train"
    "impute"
    "modal-metrics"
)

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
  -f, --from-stage <STAGE>        Start/resume at <STAGE> (skips earlier stages, re-runs from <STAGE> onward)
      --after-stage <STAGE>       Resume immediately after <STAGE> (treats <STAGE> and earlier as completed)
      --force                     Re-run all stages from the beginning, ignoring previously completed stages
  -h, --help                      Show this help message

Valid stages (in order):
  ${STAGES[*]}
EOF
}

stage_index() {
    local target="$1"
    local i
    for i in "${!STAGES[@]}"; do
        if [[ "${STAGES[$i]}" == "${target}" ]]; then
            echo "${i}"
            return 0
        fi
    done
    return 1
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
        -f|--from-stage)
            FROM_STAGE="$2"
            shift 2
            ;;
        --after-stage|--completed-stage)
            AFTER_STAGE="$2"
            shift 2
            ;;
        --force)
            FORCE_RERUN=1
            shift
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

if [[ -n "${FROM_STAGE}" && -n "${AFTER_STAGE}" ]]; then
    echo "Error: Cannot specify both --from-stage and --after-stage" >&2
    exit 1
fi

START_STAGE_IDX=0
if [[ -n "${FROM_STAGE}" ]]; then
    if ! START_STAGE_IDX="$(stage_index "${FROM_STAGE}")"; then
        echo "Error: Invalid stage '${FROM_STAGE}'. Valid stages are: ${STAGES[*]}" >&2
        exit 1
    fi
elif [[ -n "${AFTER_STAGE}" ]]; then
    if ! AFTER_IDX="$(stage_index "${AFTER_STAGE}")"; then
        echo "Error: Invalid stage '${AFTER_STAGE}'. Valid stages are: ${STAGES[*]}" >&2
        exit 1
    fi
    START_STAGE_IDX=$((AFTER_IDX + 1))
fi

if [[ -z "${CENSUS_API_KEY:-}" ]]; then
    echo "Error: Missing required environment variable CENSUS_API_KEY" >&2
    exit 1
fi

# Resolve GTFS_MANIFEST to an absolute path so subshells that `cd` can read it
GTFS_MANIFEST="$(cd "$(dirname "${GTFS_MANIFEST}")" && pwd)/$(basename "${GTFS_MANIFEST}")"

# Derived paths inside OUTPUT_DIR
mkdir -p "${OUTPUT_DIR}"
STATE_FILE="${OUTPUT_DIR}/.completed_stages"
GTFS_RAW_DIR="${OUTPUT_DIR}/gtfs"
TRANSIT_DIR="${OUTPUT_DIR}/transit"
CENSUS_CSV="${OUTPUT_DIR}/census_tracts_bamcensus.csv.gz"
CALIBRATION_DIR="${OUTPUT_DIR}/congestion_calibration"
MODELS_DIR="${OUTPUT_DIR}/congestion_models"

if [[ "${FORCE_RERUN}" -eq 1 ]]; then
    rm -f "${STATE_FILE}"
elif [[ -f "${STATE_FILE}" && ( -n "${FROM_STAGE}" || -n "${AFTER_STAGE}" ) ]]; then
    # Remove stages at or after START_STAGE_IDX from STATE_FILE so they re-run
    for (( i=START_STAGE_IDX; i<${#STAGES[@]}; i++ )); do
        stage_to_clear="${STAGES[$i]}"
        if grep -qxF "${stage_to_clear}" "${STATE_FILE}" 2>/dev/null; then
            grep -vxF "${stage_to_clear}" "${STATE_FILE}" > "${STATE_FILE}.tmp" || true
            mv "${STATE_FILE}.tmp" "${STATE_FILE}"
        fi
    done
fi

is_stage_completed() {
    local stage="$1"
    [[ -f "${STATE_FILE}" ]] && grep -qxF "${stage}" "${STATE_FILE}"
}

should_run_stage() {
    local stage="$1"
    local idx
    idx="$(stage_index "${stage}")"
    if [[ "${idx}" -lt "${START_STAGE_IDX}" ]]; then
        echo "[${stage}] Skipping (prior to start stage)"
        return 1
    fi
    if is_stage_completed "${stage}"; then
        echo "[${stage}] Skipping (already completed in ${STATE_FILE})"
        return 1
    fi
    echo "[${stage}] Running..."
    return 0
}

mark_stage_completed() {
    local stage="$1"
    if ! is_stage_completed "${stage}"; then
        echo "${stage}" >> "${STATE_FILE}"
    fi
    echo "[${stage}] Completed."
}

# 1. download OvertureMaps network dataset
if should_run_stage "network"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-omf" network \
        --configuration-file "${NETWORK_CONFIG}" \
        --extent-file "${EXTENT_FILE}" \
        --output-directory "${OUTPUT_DIR}"
    mark_stage_completed "network"
fi

# 2. download GTFS dataset
if should_run_stage "gtfs-download"; then
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
    mark_stage_completed "gtfs-download"
fi

# 3. import GTFS archives into vectorized routing table
if should_run_stage "gtfs-import"; then
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
    mark_stage_completed "gtfs-import"
fi

# 4. inject transit mode into config file
if should_run_stage "gtfs-config"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam_util" gtfs-config \
        --directory "${TRANSIT_DIR}"/transit \
        --base-config "${OUTPUT_DIR}/bambam.toml"
    mark_stage_completed "gtfs-config"
fi

# 5. collect census geometries, population, employment for congestion imputation
# requires CENSUS_API_KEY
if should_run_stage "census"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" export-census \
        --open-network-dir "${OUTPUT_DIR}" \
        --output-csv "${CENSUS_CSV}"
    mark_stage_completed "census"
fi

# 6. calibrate reference network to routing network
if should_run_stage "calibrate"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" calibrate \
        --config "${CONGESTION_CONFIG}" \
        --open-network-dir "${OUTPUT_DIR}" \
        --mode drive \
        --census-csv "${CENSUS_CSV}" \
        --reference-network-dir "${REF_NETWORK_DIR}" \
        --reference-speeds-dir "${REF_SPEEDS_DIR}" \
        --output-dir "${CALIBRATION_DIR}"
    mark_stage_completed "calibrate"
fi

# 7. train xgboost model on reference network
if should_run_stage "train"; then
    uv run python python/nlr/bambam/congestion/train_congestion_models.py \
        --calibration-dir "${CALIBRATION_DIR}" \
        --output-dir "${MODELS_DIR}"
    mark_stage_completed "train"
fi

# 8. impute speeds from xgboost model into routing network representation
if should_run_stage "impute"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" impute \
        --config "${CONGESTION_CONFIG}" \
        --open-network-dir "${OUTPUT_DIR}" \
        --mode drive \
        --census-csv "${CENSUS_CSV}" \
        --models-dir "${MODELS_DIR}" \
        --output-dir "${OUTPUT_DIR}/drive"
    mark_stage_completed "impute"
fi

# 9. calculate WCI for walk mode and LTS for bike mode
if should_run_stage "modal-metrics"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam_util" modal_metric \
        --modal-metric WCI \
        --network-dir "${OUTPUT_DIR}" \
        --output-file "${OUTPUT_DIR}/walk/wci.csv.gz"

    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam_util" modal_metric \
        --modal-metric LTS \
        --network-dir "${OUTPUT_DIR}" \
        --output-file "${OUTPUT_DIR}/bike/lts.csv.gz"
    mark_stage_completed "modal-metrics"
fi

echo "missing: injecting WCI/LTS logic into config; injecting transit-mode constraints; setting drive-mode speeds file."
echo ""
echo "finished."