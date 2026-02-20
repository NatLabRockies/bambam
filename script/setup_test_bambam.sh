#!/bin/sh

# setup.sh
# This script sets up the developer environment for the bambam project. this is
# intended to be run once after cloning the repository.

set -e


# create a virtual environment
echo "downloading boulder and denver scenarios"
uv run --with osmnx --with "nlr.routee.compass[all]" script/setup_test_bambam.py

echo "downloading University of Colorado Boulder GTFS archive"
# mkdir boulder_co # this directory already exists, created during the setup_test_bambam.py script
curl https://files.mobilitydatabase.org/mdb-181/mdb-181-202509240015/mdb-181-202509240015.zip -o boulder_co/ucb-gtfs.zip
