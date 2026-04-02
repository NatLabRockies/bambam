# BAMBAM Tasks - MVP

This document lists independent tasks that can be run during a BAMBAM run. These are written for some orchestration tool such as [Consist](https://github.com/LBNL-UCB-STI/consist). Optional arguments are shown with braces `[]`.

- Import network via `bambam-omf network` command:
  - desc: downloads OvertureMaps network for region within extent + buffer
  - inputs: `region_name, extent, [buffer_radius, modes_config, omf_version, use_slurm]`
  - outputs: `network_path`
- Create network data config:
  - desc: adds network data import metadata to compass config
  - inputs: `network_path`
  - outputs: `network config fragment`
- Import opportunity data via `bambam-omf poi`:
  - desc: downloads OvertureMaps points of interest
  - inputs: `extent [, buffer_radius, category_mapping, use_slurm]`
  - outputs: `poi_path`
- Create opportunity data config:
  - desc: adds opportunity data import metadata to compass config
  - inputs: `extent [, buffer_radius, category_mapping]`
  - outputs: `poi config fragment`
- Download transit agencies via `bambam-gtfs download`:
  - desc: runs the download over some manifest of archives
  - inputs: `extent [, use_slurm]`
  - outputs: `gtfs_directory`
- Import transit agencies via `bambam-gtfs preprocess-bundle`
  - desc: processes each archive into a searchable edge list format with metadata
  - inputs: `gtfs_directory, date, [, time_range, extent, use_slurm]`
  - outputs: `processed_gtfs_directory`
- Create GTFS data config:
  - desc: creates the edge list configuration to reference some processed GTFS data
  - inputs: `processed_gtfs_directory [, starting_edge_list_id]`
  - outputs: `gtfs config fragment`
- Create population data config:
  - desc: creates the grid input plugin entry with ACS population configuration
  - inputs: `acs_year [acs_type, acs_categories, h3_resolution]`
  - outputs: `grid input plugin config fragment`
- Create base Compass config:
  - desc: reads a version of the base config TOML file and writes it to the working compass config path
  - inputs: `[version]`
  - outputs: `compass_config_path`
- Append MEP computation to configuration (via routee-compass eval plugin)
- Run BAMBAM

This uses the following defaults:
  - buffer_radius: 40km (~40 minutes limit at ~ 40mph)
  - modes_config: configuration/bambam-omf/travel-mode-filter.json
  - omf_version: latest
  - use_slurm: false
  - category_mapping: None (uses built-in hard-coded value)
  - time_range: 8-9am
  - starting_edge_list_id: 3 (for transit edge lists, as 0=walk,1=bike,2=drive)
  - acs_type: five_year
  - acs_categories = ["B0100101E"]
  - h3_resolution = 8 (~ 1km^2)
  - version = v1

### Additional steps

- Import network via `bambam-osm`:
- Import POI from Census via `bambam`:
- Walk Comfort Index (WCI) for network:
- Level of Traffic Stress (LTS) for network:
- Congestion-based Traffic Speeds for network:
