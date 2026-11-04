//! Validated `Agency` model.

use crate::ids::AgencyId;
use crate::raw::RawAgency;

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
    /// Raw IANA timezone string where the transit agency is located.
    pub timezone: String,
    /// Primary language used by this transit agency.
    pub lang: Option<String>,
    /// Voice telephone number for the specified agency.
    pub phone: Option<String>,
    /// URL of a web page that allows a rider to purchase tickets online.
    pub fare_url: Option<String>,
    /// Customer service email address.
    pub email: Option<String>,
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
