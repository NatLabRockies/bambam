//! state feature names assigned to the state model in bambam runs. also exports
//! the upstream-defined features from compass.

/// time delays accumulated throughout the trip
pub const TRIP_ENROUTE_DELAY: &str = "trip_enroute_delay";

/// time delays on arriving at a destination, such as parking, which
/// are not incorporated into the search cost function.
pub const TRIP_ARRIVAL_DELAY: &str = "trip_arrival_delay";

/// during scheduled mode traversals, a record of the route used.
pub const ROUTE_ID: &str = "route_id";

/// a record of the total "switching mode" time. currently used in transit traversal to model waiting time
pub const TRANSIT_BOARDING_TIME: &str = "transit_boarding_time";
/// a record of the total time sitting on transit during dwell in between edge traversals.
pub const DWELL_TIME: &str = "dwell_time";

/// used to penalize an edge. convention is to design this
/// as one of the vehicle cost rates, via a "raw" interpretation
/// (no cost conversion) and then to use "mul" (multiplicitive)
/// cost aggregation with this value and the total edge time.
/// when this value is 1.0, no penalty is applied.
/// if it is < 1, it reduces cost, and > 1, increases cost.
pub const COST_PENALTY_FACTOR: &str = "penalty_factor";

pub use routee_compass_core::model::traversal::default::fieldname::*;
pub use routee_compass_powertrain::model::fieldname::*;
