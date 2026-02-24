# BAMBAM

<div align="left">
  <a href="https://crates.io/crates/bambam">
    <img src="https://img.shields.io/crates/v/bambam" alt="Crates.io Latest Release"/>
  </a>
</div>

Access modeling toolkit for Rust built on the RouteE Compass energy-aware route planner.

The Behavior and Advanced Mobility Big Access Model (BAMBAM) is a mobility research platform for scalable access modeling. The process begins with a grid defined at some spatial granularity (e.g., census block or 1 km grid) and a variety of travel configurations. For each grid cell and configuration, the platform executes constrained searches, uses the results to index points of interest (POI), and aggregates the findings to the grid level. It provides researchers access through R or Python running on HPC. The platform automates the import and merging of datasets from a variety of sources including data.gov (with automatic merging of Tiger/Lines geometries), OpenStreetMaps, OvertureMaps, and GTFS. It is built upon [RouteE Compass](https://github.com/nlr/routee-compass), a scalable, energy-aware route planner written in Rust, extended to model multiple travel modes.

This software is in a [**beta**](https://en.wikipedia.org/wiki/Software_release_life_cycle#Beta) phase of development. 

# Usage

BAMBAM is provided as either a set of command line tools or via a Python API. 

## CLI

First, install using cargo (via [rustup](rustup.rs)): `cargo build --release --manifest-path rust/Cargo.toml`. Compilation generates the following command line utilities:

  - **bambam** - runs the access model
  - **bambam_util** - runs specific support utilities
  - **bambam_gtfs** - [GTFS](https://gtfs.org/documentation/schedule/reference/) analysis and import script
  - **bambam_osm** - [OpenStreetMap](https://www.openstreetmap.org/) import script
  - **bambam_omf** - [OvertureMaps](https://docs.overturemaps.org/) import script

We can list the command arguments for bambam (will document app as "RouteE Compass"):

```
$ ./rust/target/release/bambam --help
The RouteE-Compass energy-aware routing engine

Usage: bambam [OPTIONS] --config-file <*.toml> --query-file <*.json>

Options:
  -c, --config-file <*.toml>   RouteE Compass service configuration TOML file
  -q, --query-file <*.json>    JSON file containing queries. Should be newline-delimited if chunksize is set
      --chunksize <CHUNKSIZE>  Size of batches to load into memory at a time
  -n, --newline-delimited      Format of JSON queries file, if regular JSON or newline-delimited JSON
  -h, --help                   Print help
  -V, --version                Print version
```

## Python

Install the library, currently unpublished, using pip: `pip install -e ".[all]"`.

# Example

Before running any examples, run the following setup command(s) to download example data assets:

``` sh
$ ./script/setup_test_bambam.sh
$ ./script/setup_test_bambam_osm.sh
```

### Denver

This test uses drive-mode traversal to report opportunities in the Denver Metro region:

```
$ RUST_LOG=info ./rust/target/release/bambam --config-file denver_co/denver_test.toml --query-file query/denver_extent.json
```

or, in Python:

```python
from nlr.bambam import BambamRunner
import pandas
import json

app = BambamRunner.from_config_file("configuration/test_denver.toml")
with open("query/denver_extent.json") as f:
    query = json.loads(f.read())
result = app.run(query)
```
### Boulder

This test uses walk-transit traversal to report opportunities near University of Colorado Boulder. First, process the GTFS archive:

```sh
$ RUST_LOG=info ./configuration/bambam-gtfs/local-exact-range.sh
```

Now run BAMBAM using the boulder_test.toml configuration file and a small study area extent:

```sh
$ RUST_LOG=info ./rust/target/release/bambam --config-file configuration/boulder_test.toml --query-file query/boulder_broadway_and_euclid.json
```

# Roadmap

- [x] Python API
- [ ] R API
- [ ] OvertureMaps network import
- [ ] methodological improvements for walk/bike/drive realism
- [x] transit-mode travel using GTFS Schedule data networks
- [x] multimodal route planning

# License

Copyright 2025 Alliance for Energy Innovation, LLC

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS “AS IS” AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
