#!/bin/sh

set -e

echo "running bambam-transit to generate Compass datasets and metadata from a GTFS archive"
./configuration/bambam-transit/local-match-nearest-date.sh  
echo "running gtfs-config to modify an existing BAMBAM TOML configuration with this transit dataset"
./rust/target/release/bambam_util gtfs-config --directory boulder_co/transit --base-config configuration/test_gtfs_config_boulder.toml
echo "running BAMBAM with a walk-transit trip"
./rust/target/release/bambam -c configuration/test_gtfs_config_boulder_gtfs.toml -q query/boulder_broadway_and_euclid.json   
