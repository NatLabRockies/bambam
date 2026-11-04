//! Validated `Agency` model.

use crate::ids::AgencyId;
use crate::primitives::{deserialize_timezone, serialize_timezone, RkyvTz};
use crate::raw::RawAgency;
use chrono_tz::Tz;

/// A validated transit agency (`agency.txt`).
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize,
    rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub struct Agency {
    /// Optional unique agency identifier (mandatory when multiple agencies exist).
    pub id: Option<AgencyId>,
    /// Full name of the transit agency.
    pub name: String,
    /// URL of the transit agency.
    pub url: String,
    /// Strongly-typed IANA [`chrono_tz::Tz`] timezone where the transit agency is located (`agency_timezone`).
    #[serde(
        deserialize_with = "deserialize_timezone",
        serialize_with = "serialize_timezone"
    )]
    #[rkyv(with = RkyvTz)]
    pub timezone: Tz,
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
    /// Returns the IANA [`chrono_tz::Tz`] timezone for this agency.
    #[inline]
    pub const fn time_zone(&self) -> Tz {
        self.timezone
    }
}

impl From<RawAgency> for Agency {
    fn from(raw: RawAgency) -> Self {
        Self {
            id: raw.id.filter(|id| !id.is_empty()),
            name: raw.name,
            url: raw.url,
            timezone: raw.timezone,
            lang: raw.lang,
            phone: raw.phone,
            fare_url: raw.fare_url,
            email: raw.email,
        }
    }
}
