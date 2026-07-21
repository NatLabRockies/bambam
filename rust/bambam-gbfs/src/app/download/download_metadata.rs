use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Clone, Debug)]
pub enum UnversionedGbfsVersion {
    #[serde(rename = "2.3")]
    V2_3,
    #[serde(rename = "3.0")]
    V3_0,
}

impl Serialize for UnversionedGbfsVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            UnversionedGbfsVersion::V2_3 => serializer.serialize_str("2.3"),
            UnversionedGbfsVersion::V3_0 => serializer.serialize_str("3.0"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnversionedGbfsMetadata {
    pub last_updated: Value,
    pub ttl: Value,
    pub version: UnversionedGbfsVersion,
}
