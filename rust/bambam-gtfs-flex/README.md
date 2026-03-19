# bambam-gtfs-flex
A set of extensions that enable On-Demand Transit modeling in BAMBAM using [GTFS-Flex](https://gtfs.org/community/extensions/flex/) datasets.

## GTFS Flex Service Types

We have observed four types of services:

1. **Within a Single Zone**  
  Pickups and drop-offs at any two points within the same zone.

2. **Across Multiple Zones**  
  Trips are allowed between any points in different zones (no within-zone trips).

3. **With Specified Stops**  
  Similar to services (1) and (2), but pickups and drop-offs are allowed only at designated stops.

4. **With Deviated Route**  
  Vehicles follow a fixed route but can deviate to pick up or drop off between stops.

## Model Design

To track the state of a GTFS Flex trip, we store a few additional fields on the trip state. These fields and their implications in service types 1-3 are described below. Note: service type 4 is implemented via network dataset modifications and not in the search algorithm state.

### Service Type 1: Within a Single Zone

In this service type, trips are assigned a `src_zone_id` when they board. The trip may travel anywhere but may only treat locations within this zone as destinations. This requires a lookup table from `EdgeId -> ZoneId`.

### Service Type 2: Across Multiple Zones

In this service type, trips are assigned a `src_zone_id` and `departure_time` when they board. The trip may travel anywhere but may only treat particular locations as destinations. This requires the above `EdgeId -> ZoneId` lookup as well as a `(ZoneId, DepartureTime) -> [ZoneId]` lookup.

### Service Type 3: With Specified Stops

In this service type, trips are assigned a `src_zone_id` and `departure_time` when they board. Using the same lookups as (2), we additionally require an `EdgeId -> bool` mask function which further restricts where boarding and alighting may occur.

### Service Type 4: With Deviated Route

In this service type, we are actually running GTFS-style routing. However, we also need to modify some static weights based on the expected delays due to trip deviations. These weights should be modified during trip/model initialization but made fixed to ensure search correctness.

## Processing GTFS Flex Feeds Using CLI

To process GTFS Flex feeds, you can use the provided command-line interface (CLI) tool. Follow the steps below:

1. **Install Dependencies**  
  Ensure you have Rust installed on your system. If not, install it from [rustup.rs](https://rustup.rs/).

2. **Build the Project**  
  Navigate to the project directory and build the CLI tool:
  ```bash
  cd rust/bambam-gtfs-flex
  cargo build --release
  ```

3. **Run the CLI Tool**  
  Use the following command to process a GTFS Flex feed:
  ```bash
  ./target/release/bambam-gtfs-flex process-feeds ./src/test/assets 20240903 valid_zone.csv
  ```
  If you want to process GTFS-Flex feeds without completing `cargo build --release` in Step 2, you can simply use the following command:
  ```bash
  cd rust/bambam-gtfs-flex
  cargo run -- process-feeds ./src/test/assets 20240903
  ```
  Replace `./src/test/assets` with the path to the folder where your GTFS-Flex feeds (.zip files) are located, `20240903` with the desired date in `YYYYMMDD` format for which you want to process the feeds, and `valid_zones.csv` (optional) with the name of the output CSV file, which will be written to the GTFS-Flex feeds directory.

4. **Verify Output**  
  After processing, the output directory will contain the processed valid zone CSV for the requested date, ready for use in BAMBAM.

Refer to the project's documentation for more details on further usage and configuration.
