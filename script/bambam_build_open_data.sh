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
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="${REPO_ROOT:-${DEFAULT_REPO_ROOT}}"
NETWORK_CONFIG="${NETWORK_CONFIG:-}"
CONGESTION_CONFIG="${CONGESTION_CONFIG:-}"
EXTENT_FILE="${EXTENT_FILE:-}"
OUTPUT_DIR="${OUTPUT_DIR:-}"
GTFS_MANIFEST="${GTFS_MANIFEST:-}"
GTFS_SUBDIVISION="${GTFS_SUBDIVISION:-}"
REF_NETWORK_DIR="${REF_NETWORK_DIR:-}"
REF_SPEEDS_DIR="${REF_SPEEDS_DIR:-}"
LOCAL_OMF_DIR="${LOCAL_OMF_DIR:-}"
DEFAULT_CPUS="${SLURM_CPUS_PER_TASK:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 16)}"
PARALLELISM="${PARALLELISM:-${DEFAULT_CPUS}}"
DOWNLOAD_PARALLELISM="${DOWNLOAD_PARALLELISM:-16}"
FROM_STAGE="${FROM_STAGE:-}"
AFTER_STAGE="${AFTER_STAGE:-}"
TO_STAGE="${TO_STAGE:-}"
FORCE_RERUN=0

STAGES=(
    "network"
    "gtfs-download"
    "gtfs-import"
    "gtfs-config"
    "census"
    "opportunities"
    "calibrate"
    "train"
    "impute"
    "modal-metrics"
)

resolve_abs_path() {
    local target="$1"
    if [[ -d "${target}" ]]; then
        (cd "${target}" && pwd)
    else
        (cd "$(dirname "${target}")" && echo "$(pwd)/$(basename "${target}")")
    fi
}

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Required arguments:
  -e, --extent-file <FILE>        WKT extent file (e.g., us-all-wkt.txt)
  -o, --output-dir <DIR>          Output network directory (e.g., /scratch/\$USER/us_network)
  -m, --gtfs-manifest <FILE>      Mobility Data Catalog CSV file for GTFS download
  -r, --ref-network-dir <DIR>     Reference road network directory (e.g., TomTom Compass dataset)

Optional arguments:
  -s, --ref-speeds-dir <DIR>      Directory with reference time-bin speed files (defaults to --ref-network-dir)
  -d, --subdivision-name <NAME>   Optional state/subdivision filter for GTFS download (e.g., "Colorado")
      --local-omf-dir <DIR>       Optional local Overture Maps Parquet directory to avoid S3 downloads
      --repo-root <DIR>           Path to bambam repository root (default: ${REPO_ROOT})
  -n, --network-config <FILE>     Network import config (default: <REPO_ROOT>/configuration/bambam-network/bambam-network-config.json)
  -c, --congestion-config <FILE>  Congestion pipeline config (default: <REPO_ROOT>/configuration/bambam-congestion/congestion.toml)
  -p, --parallelism <N>           CPU parallelism for import, Rust Rayon, and model training (default: ${PARALLELISM})
      --download-parallelism <N>  Parallelism for GTFS HTTP downloads (default: ${DOWNLOAD_PARALLELISM})
  -f, --from-stage <STAGE>        Start/resume at <STAGE> (skips earlier stages, re-runs from <STAGE> onward)
      --after-stage <STAGE>       Resume immediately after <STAGE> (treats <STAGE> and earlier as completed)
  -t, --to-stage <STAGE>          Stop after completing <STAGE> (useful for splitting download vs. compute stages)
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
        --local-omf-dir|--omf-local-source-dir)
            LOCAL_OMF_DIR="$2"
            shift 2
            ;;
        --repo-root)
            REPO_ROOT="$2"
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
        -p|--parallelism)
            PARALLELISM="$2"
            shift 2
            ;;
        --download-parallelism)
            DOWNLOAD_PARALLELISM="$2"
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
        -t|--to-stage)
            TO_STAGE="$2"
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

# Resolve REPO_ROOT and default config files anchored to REPO_ROOT
if [[ ! -d "${REPO_ROOT}" ]]; then
    echo "Error: Repository root directory '${REPO_ROOT}' does not exist." >&2
    exit 1
fi
REPO_ROOT="$(cd "${REPO_ROOT}" && pwd)"
NETWORK_CONFIG="${NETWORK_CONFIG:-${REPO_ROOT}/configuration/bambam-network/bambam-network-config.json}"
CONGESTION_CONFIG="${CONGESTION_CONFIG:-${REPO_ROOT}/configuration/bambam-congestion/congestion.toml}"

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

END_STAGE_IDX=$((${#STAGES[@]} - 1))
if [[ -n "${TO_STAGE}" ]]; then
    if ! END_STAGE_IDX="$(stage_index "${TO_STAGE}")"; then
        echo "Error: Invalid stage '${TO_STAGE}'. Valid stages are: ${STAGES[*]}" >&2
        exit 1
    fi
fi

if [[ "${START_STAGE_IDX}" -gt "${END_STAGE_IDX}" ]]; then
    echo "Error: Start stage comes after --to-stage '${TO_STAGE}'" >&2
    exit 1
fi

# Validate input files/directories exist before resolving to absolute paths
for file_var in EXTENT_FILE GTFS_MANIFEST NETWORK_CONFIG CONGESTION_CONFIG; do
    if [[ ! -f "${!file_var}" ]]; then
        echo "Error: ${file_var} file not found: '${!file_var}'" >&2
        exit 1
    fi
done
for dir_var in REF_NETWORK_DIR REF_SPEEDS_DIR; do
    if [[ ! -d "${!dir_var}" ]]; then
        echo "Error: ${dir_var} directory not found: '${!dir_var}'" >&2
        exit 1
    fi
done
if [[ -n "${LOCAL_OMF_DIR}" && ! -d "${LOCAL_OMF_DIR}" ]]; then
    echo "Error: LOCAL_OMF_DIR directory not found: '${LOCAL_OMF_DIR}'" >&2
    exit 1
fi

# Validate compiled Rust release binaries exist under REPO_ROOT
BIN_DIR="${REPO_ROOT}/rust/target/release"
for bin_name in bambam-omf bambam-transit bambam_util bambam-congestion; do
    if [[ ! -x "${BIN_DIR}/${bin_name}" ]]; then
        echo "Error: Required binary '${BIN_DIR}/${bin_name}' not found or not executable." >&2
        echo "Hint: Build release binaries first with: cargo build --release --manifest-path \"${REPO_ROOT}/rust/Cargo.toml\"" >&2
        exit 1
    fi
done

# Normalize paths to absolute so subshells or working directories in /scratch work reliably
mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(resolve_abs_path "${OUTPUT_DIR}")"
EXTENT_FILE="$(resolve_abs_path "${EXTENT_FILE}")"
GTFS_MANIFEST="$(resolve_abs_path "${GTFS_MANIFEST}")"
REF_NETWORK_DIR="$(resolve_abs_path "${REF_NETWORK_DIR}")"
REF_SPEEDS_DIR="$(resolve_abs_path "${REF_SPEEDS_DIR}")"
NETWORK_CONFIG="$(resolve_abs_path "${NETWORK_CONFIG}")"
CONGESTION_CONFIG="$(resolve_abs_path "${CONGESTION_CONFIG}")"
if [[ -n "${LOCAL_OMF_DIR}" ]]; then
    LOCAL_OMF_DIR="$(resolve_abs_path "${LOCAL_OMF_DIR}")"
fi

# Configure Rayon thread pool to match Slurm / CLI parallelism
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-${PARALLELISM}}"
STATE_FILE="${OUTPUT_DIR}/.completed_stages"
GTFS_RAW_DIR="${OUTPUT_DIR}/gtfs"
TRANSIT_DIR="${OUTPUT_DIR}/transit"
CENSUS_CSV="${OUTPUT_DIR}/census_tracts_bamcensus.csv.gz"
OPPORTUNITIES_CSV="${OUTPUT_DIR}/opportunities_omf.csv.gz"
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
    if [[ "${idx}" -gt "${END_STAGE_IDX}" ]]; then
        echo "[${stage}] Skipping (after --to-stage '${TO_STAGE}')"
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
    NETWORK_ARGS=(
        --configuration-file "${NETWORK_CONFIG}"
        --extent-file "${EXTENT_FILE}"
        --output-directory "${OUTPUT_DIR}"
    )
    if [[ -n "${LOCAL_OMF_DIR}" ]]; then
        NETWORK_ARGS+=(--local-source "${LOCAL_OMF_DIR}")
    fi
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-omf" network "${NETWORK_ARGS[@]}"
    mark_stage_completed "network"
fi

# 2. download GTFS dataset
if should_run_stage "gtfs-download"; then
    mkdir -p "${GTFS_RAW_DIR}"
    DOWNLOAD_ARGS=(
        --manifest-file "${GTFS_MANIFEST}"
        --country-code US
        --data-type gtfs
        --parallelism "${DOWNLOAD_PARALLELISM}"
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
        --parallelism "${PARALLELISM}" \
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
    # Run from OUTPUT_DIR in a subshell so CompassAppConfig path normalization and
    # gtfs-config both emit paths relative to OUTPUT_DIR (e.g., "walk/...", "transit/...")
    # rather than baking the OUTPUT_DIR prefix into bambam_gtfs.toml.
    (
        cd "${OUTPUT_DIR}" && \
        RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam_util" gtfs-config \
            --directory "transit" \
            --base-config "bambam.toml"
    )
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

# 6. collect MEP land-use opportunities once so calibrate and impute do not re-download from S3
if should_run_stage "opportunities"; then
    OPP_ARGS=(
        --config "${CONGESTION_CONFIG}"
        --open-network-dir "${OUTPUT_DIR}"
        --output-csv "${OPPORTUNITIES_CSV}"
    )
    if [[ -n "${LOCAL_OMF_DIR}" ]]; then
        OPP_ARGS+=(--omf-local-source-dir "${LOCAL_OMF_DIR}")
    fi
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" export-opportunities "${OPP_ARGS[@]}"
    mark_stage_completed "opportunities"
fi

# 7. calibrate reference network to routing network
if should_run_stage "calibrate"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" calibrate \
        --config "${CONGESTION_CONFIG}" \
        --open-network-dir "${OUTPUT_DIR}" \
        --mode drive \
        --census-csv "${CENSUS_CSV}" \
        --opportunities-csv "${OPPORTUNITIES_CSV}" \
        --reference-network-dir "${REF_NETWORK_DIR}" \
        --reference-speeds-dir "${REF_SPEEDS_DIR}" \
        --output-dir "${CALIBRATION_DIR}"
    mark_stage_completed "calibrate"
fi

# 8. train xgboost model on reference network
if should_run_stage "train"; then
    TRAIN_SCRIPT="${REPO_ROOT}/python/nlr/bambam/congestion/train_congestion_models.py"
    if [[ -x "${REPO_ROOT}/.venv/bin/python" ]]; then
        "${REPO_ROOT}/.venv/bin/python" "${TRAIN_SCRIPT}" \
            --calibration-dir "${CALIBRATION_DIR}" \
            --output-dir "${MODELS_DIR}" \
            --n-jobs "${PARALLELISM}"
    else
        UV_CACHE_DIR="${UV_CACHE_DIR:-${OUTPUT_DIR}/.uv-cache}" \
            uv run --project "${REPO_ROOT}" --no-sync python "${TRAIN_SCRIPT}" \
            --calibration-dir "${CALIBRATION_DIR}" \
            --output-dir "${MODELS_DIR}" \
            --n-jobs "${PARALLELISM}"
    fi
    mark_stage_completed "train"
fi

# 9. impute speeds from xgboost model into routing network representation
if should_run_stage "impute"; then
    RUST_LOG=info "${REPO_ROOT}/rust/target/release/bambam-congestion" impute \
        --config "${CONGESTION_CONFIG}" \
        --open-network-dir "${OUTPUT_DIR}" \
        --mode drive \
        --census-csv "${CENSUS_CSV}" \
        --opportunities-csv "${OPPORTUNITIES_CSV}" \
        --models-dir "${MODELS_DIR}" \
        --output-dir "${OUTPUT_DIR}/drive"
    mark_stage_completed "impute"
fi

# 10. calculate WCI for walk mode and LTS for bike mode
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