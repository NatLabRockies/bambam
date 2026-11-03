#!/bin/sh

# setup_test_bambam.sh
# Regenerates the boulder_co test fixtures from OSM and Mobility Database.
# Note: boulder_co is committed to the repository, so running this script
# is only necessary when refreshing the committed test assets.

set -e

echo "downloading boulder scenario"
uv run --with osmnx --with "nrel.routee.compass[all]" script/setup_test_bambam.py

echo "downloading University of Colorado Boulder GTFS archive"
curl https://files.mobilitydatabase.org/mdb-181/mdb-181-202509240015/mdb-181-202509240015.zip -o boulder_co/ucb-gtfs.zip

