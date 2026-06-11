use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use regex::Regex;

use crate::util::zone::ZoneError;

/// represents a zone in a GTFS Flex agency. the meaning of the zone
/// value depends on the service type of the agency.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ZoneId(String);

impl std::fmt::Display for ZoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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

impl TryFrom<&str> for ZoneId {
    type Error = ZoneError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if zone_id_regex().is_match(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(ZoneError::Build(format!(
                "invalid zone id '{value}'; expected format matching '{ZONE_ID_REGEX_LITERAL}'"
            )))
        }
    }
}

/// expected pattern for a zone id, which is a hyphen-delimited sequence of strings
/// of the form `{agency_id}-{route_id}-{trip_id}-{location_id}`.
const ZONE_ID_REGEX_LITERAL: &str = r".+-.+-.+-.+";

/// ensure we compile the regex exactly once per run of this program.
static ZONE_ID_REGEX: OnceLock<Regex> = OnceLock::new();

/// gets the regex stored in the OnceLock for use in validating ZoneId values.
fn zone_id_regex() -> &'static Regex {
    ZONE_ID_REGEX.get_or_init(|| {
        Regex::new(&format!("^{ZONE_ID_REGEX_LITERAL}$"))
            .expect("zone id regex literal must compile")
    })
}
