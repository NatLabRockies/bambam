#!/bin/sh

set -eu

MANIFEST_PATH="rust/Cargo.toml"
DRY_RUN=0
CRATES_CSV=""
DEFAULT_CRATES="bambam-core bambam-osm bambam-gtfs bambam-gbfs bambam-omf bambam-gtfs-flex bambam"

usage() {
  cat <<'EOF'
Usage: script/publish_crates.sh [--dry-run] [--manifest-path <path>] [--crates <comma-delimited-names>]

Publishes bambam crates to crates.io in dependency order.
Use --dry-run to validate each publish command without uploading.
Use --crates to run a comma-delimited subset (for reruns after partial failure).
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      ;;
    --manifest-path)
      shift
      if [ "$#" -eq 0 ]; then
        echo "missing value for --manifest-path" >&2
        usage
        exit 2
      fi
      MANIFEST_PATH="$1"
      ;;
    --crates)
      shift
      if [ "$#" -eq 0 ]; then
        echo "missing value for --crates" >&2
        usage
        exit 2
      fi
      CRATES_CSV="$1"
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage
      exit 2
      ;;
  esac
  shift
done

if [ -n "$CRATES_CSV" ]; then
  # Normalize comma-delimited input by removing whitespace.
  CRATES_CSV="$(printf '%s' "$CRATES_CSV" | tr -d '[:space:]')"
  if [ -z "$CRATES_CSV" ]; then
    echo "--crates cannot be empty" >&2
    exit 2
  fi
  CRATES="$(printf '%s' "$CRATES_CSV" | tr ',' ' ')"
else
  CRATES="$DEFAULT_CRATES"
fi

run_publish() {
  crate="$1"
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "dry-run publish for ${crate}"
    # --no-verify avoids false failures when sibling crate versions are not yet on crates.io.
    cargo publish -p "$crate" --manifest-path="$MANIFEST_PATH" --dry-run --no-verify
  else
    echo "publishing ${crate}"
    cargo publish -p "$crate" --manifest-path="$MANIFEST_PATH"
  fi
}

for crate in $CRATES; do
  if [ "$DRY_RUN" -eq 0 ] && [ "$crate" = "bambam" ]; then
    # crates.io indexing can lag briefly; wait before publishing the umbrella crate.
    sleep 2
  fi
  run_publish "$crate"
done
