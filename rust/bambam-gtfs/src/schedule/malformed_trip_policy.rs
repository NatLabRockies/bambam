use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Enumerates alternative ways to handle when a trip is malformed.
#[derive(Serialize, Deserialize, Debug, ValueEnum, Clone)]
pub enum MalformedTripPolicy {
    /// if a trip has a structural issue, end the import and report failure
    Fail,
    /// if a trip has a structural issue, remove the associated Trip(s) from the Gtfs
    Drop,
}
