use serde::{Deserialize, Serialize};

/// represents a zone in a GTFS Flex agency. the meaning of the zone
/// value depends on the service type of the agency.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ZoneId(String);

impl std::fmt::Display for ZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for ZoneId {
    fn from(value: &str) -> Self {
        ZoneId(value.to_string())
    }
}

impl ZoneId {
    pub fn from_full_namespace(
        agency_id: &str,
        route_id: &str,
        trip_id: &str,
        location_id: &str,
    ) -> ZoneId {
        ZoneId(format!("{agency_id}-{route_id}-{trip_id}-{location_id}"))
    }
}
