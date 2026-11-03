use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Enumerates alternative ways to handle
/// missing lon,lat data for a stop
#[derive(Serialize, Deserialize, Debug, ValueEnum, Clone)]
pub enum MissingStopLocationPolicy {
    /// if a Stop cannot be map matched, end the import and report failure
    Fail,
    /// if a Stop cannot be map matched, remove the associated Trip(s) from the Gtfs
    Drop,
}
