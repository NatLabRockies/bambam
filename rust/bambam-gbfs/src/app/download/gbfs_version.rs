use std::str::FromStr;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// GBFS version of the targeted archive. only supported versions are included.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GbfsVersion {
    #[serde(rename = "v3.0")]
    V3_0,
}

impl GbfsVersion {
    pub const ALL: [&'static str; 1] = ["v3.0"];
}

impl std::fmt::Display for GbfsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GbfsVersion::V3_0 => write!(f, "v3.0"),
        }
    }
}

impl FromStr for GbfsVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "v3.0" => Ok(Self::V3_0),
            _ => Err(format!(
                "unknown version '{s}', must be one of [{}]",
                Self::ALL.join(", ")
            )),
        }
    }
}
