use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// GBFS version of the targeted archive. only supported versions are included.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GbfsVersion {
    #[serde(rename = "3.0")]
    V3_0,
    #[serde(rename = "2.3")]
    V2_3,
    #[serde(rename = "2.2")]
    V2_2,
}

impl GbfsVersion {
    pub const ALL: [&'static str; 3] = ["3.0", "2.3", "2.2"];
}

impl std::fmt::Display for GbfsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GbfsVersion::V3_0 => write!(f, "3.0"),
            GbfsVersion::V2_3 => write!(f, "2.3"),
            GbfsVersion::V2_2 => write!(f, "2.2"),
        }
    }
}

impl FromStr for GbfsVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "3.0" => Ok(Self::V3_0),
            "2.3" => Ok(Self::V2_3),
            "2.2" => Ok(Self::V2_2),
            _ => Err(format!(
                "unknown version '{s}', must be one of [{}]",
                Self::ALL.join(", ")
            )),
        }
    }
}
