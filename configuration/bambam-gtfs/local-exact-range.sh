#!/bin/sh

./rust/target/release/bambam_gtfs preprocess-bundle \
    --input "boulder_co/ucb-gtfs.zip" \
    --parallelism 1 \
    --date-mapping-policy exact-range \
    --output-directory "boulder_co/transit" \
    --vertices-compass-filename "boulder_co/vertices-complete.csv.gz" \
    --start-date 09-01-2025 \
    --end-date 09-01-2025