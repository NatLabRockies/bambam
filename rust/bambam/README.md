# bambam

A Rust crate for access modeling and spatial analysis, built on the RouteE Compass routing engine.

## What is Access Modeling?

Access modeling is a spatial analysis technique that measures the accessibility of locations to destinations of interest (such as jobs, services, or amenities) within a given travel time or distance threshold. It helps answer questions like:

- How many jobs can a resident reach within 30 minutes?
- What percentage of the population has access to healthcare within a 15-minute travel time?
- How does transportation infrastructure affect access to essential services?

Access models are crucial for transportation planning, equity analysis, and understanding the relationship between mobility and opportunity.

## Relationship to RouteE Compass

`bambam` is built on top of [RouteE Compass](https://github.com/NatLabRockies/routee-compass), NLR's open-source routing engine. While RouteE Compass provides efficient point-to-point routing and energy-aware navigation, `bambam` extends this functionality to perform large-scale access analysis by:

- Computing many-to-many origin-destination matrices
- Aggregating accessibility metrics across populations
- Supporting parallelized batch processing for regional analysis
- Providing specialized tools for equity and accessibility research

## Installation

Add `bambam` to your `Cargo.toml`:

```toml
[dependencies]
bambam = "0.2.3"
```

Or install via cargo:

```sh
cargo add bambam
```

## Usage

### CLI Example

See the contents of these scripts to review.

```sh
# clone the repository
git clone https://github.com/NatLabRockies/bambam
cd bambam

# bring in an example
./scripts/setup_test_bambam.sh
./scripts/setup_test_bambam_gtfs.sh

# generates result_transit.csv and result_transit.json
./scripts/run_gtfs_test.sh
```

## License

Copyright 2025 Alliance for Energy Innovation, LLC

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS “AS IS” AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Contributing

BAMBAM is currently in beta led by @robfitzgerald with contributions from @yamilbknsu, @brycemines and @evageier. Working with additional contributors will be handled on a case-by-base basis until the project reaches a later stage of maturity.