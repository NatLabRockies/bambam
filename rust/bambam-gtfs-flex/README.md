# bambam-gtfs-flex

A set of extensions that enable On-Demand Transit modeling in BAMBAM using [GTFS-Flex](https://gtfs.org/community/extensions/flex/) datasets.

## GTFS Flex Service Types

We have observed four types of services:

  1. Within a Single Zone
    - Pickups and drop-offs at any two points within the same zone
  2. Across Multiple Zones
    - Trips are allowed between any points in different zones (no within-zone trips)
  3. With Specified Stops
    - Alike (1) and (2) services, but pickups and drop-offs are allowed only at designated stops
  4. With Deviated Route
    - Vehicles follow a fixed route but can deviate to pick up or drop off between stops

## Model Design

In order to track the state of a GTFS Flex trip, we store a few additional fields on the trip state. These fields and their implications in service types 1-3 are described below. Note: service type 4 is implemented via network dataset modifications and not in search algorithm state.

### Service Type 1: Within a Single Zone

In this service type, trips are assigned a src_zone_id when they board. The trip may travel anywhere, but may only treat locations within this zone as destinations. This requires a lookup table from EdgeId -> ZoneId.

### Service Type 2: Across Multiple Zones

In this service type, trips are assigned a src_zone_id and departure_time when they board. The trip may travel anywhere, but may only treat particular locations as destinations. This requires the above `EdgeId -> ZoneId` lookup as well as a `(ZoneId, DepartureTime) -> [ZoneId]` lookup.

### Service Type 3: With Specified Stops

In this service type, trips are assigned a src_zone_id and departure_time when they board. Using the same lookups as (2), we additionally require a EdgeId -> bool mask function which further restricts where boarding and alighting may occur.

### Service Type 4: With Deviated Route

In this service type, we are actually running GTFS-style routing. However, we also need to modify some static weights based on the expected delays due to trip deviations. This weights should be modified during trip/model initialization but made fixed to ensure search correctness.
