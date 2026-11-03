//! Validated `Agency` model.

use crate::ids::AgencyId;
use crate::raw::RawAgency;
use jiff::tz::TimeZone;
use serde::de::{self, Deserialize, Deserializer};
use serde::ser::Serializer;

pub(crate) fn serialize_option_timezone<S: Serializer>(
    tz: &Option<TimeZone>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    let name: Option<&str> = tz.as_ref().and_then(|t| t.iana_name());
    name.serialize(serializer)
}

pub(crate) fn deserialize_option_timezone<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TimeZone>, D::Error> {
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => TimeZone::get(s.trim())
            .map(Some)
            .map_err(de::Error::custom),
    }
}

/// A validated transit agency (`agency.txt`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agency {
    /// Optional unique agency identifier (mandatory when multiple agencies exist).
    pub id: Option<AgencyId>,
    /// Full name of the transit agency.
    pub name: String,
    /// URL of the transit agency.
    pub url: String,
    /// Raw IANA timezone string where the transit agency is located.
    pub timezone: String,
    /// Parsed IANA [`jiff::tz::TimeZone`] for local service-day and UTC/cross-timezone normalization.
    #[serde(
        serialize_with = "serialize_option_timezone",
        deserialize_with = "deserialize_option_timezone",
        default
    )]
    pub parsed_timezone: Option<TimeZone>,
    /// Primary language used by this transit agency.
    pub lang: Option<String>,
    /// Voice telephone number for the specified agency.
    pub phone: Option<String>,
    /// URL of a web page that allows a rider to purchase tickets online.
    pub fare_url: Option<String>,
    /// Customer service email address.
    pub email: Option<String>,
}

impl Agency {
    /// Returns the parsed [`jiff::tz::TimeZone`] for this agency, if valid.
    #[inline]
    pub fn time_zone(&self) -> Option<&TimeZone> {
        self.parsed_timezone.as_ref()
    }
}

impl From<RawAgency> for Agency {
    fn from(raw: RawAgency) -> Self {
        let parsed_timezone = TimeZone::get(raw.timezone.trim()).ok();
        Self {
            id: raw.id.filter(|id| !id.is_empty()),
            name: raw.name,
            url: raw.url,
            timezone: raw.timezone,
            parsed_timezone,
            lang: raw.lang,
            phone: raw.phone,
            fare_url: raw.fare_url,
            email: raw.email,
        }
    }
}
