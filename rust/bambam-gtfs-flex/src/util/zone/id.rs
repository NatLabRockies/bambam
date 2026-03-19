use serde::{Deserialize, Serialize};

/// represents a zone in a GTFS Flex agency. the meaning of the zone
/// value depends on the service type of the agency.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ZoneId(pub String);

impl std::fmt::Display for ZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
