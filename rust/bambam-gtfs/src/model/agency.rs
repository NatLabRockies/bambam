//! Validated `Agency` model.

use crate::ids::AgencyId;
use crate::primitives::{deserialize_timezone, serialize_timezone, RkyvTz};
use crate::raw::RawAgency;
use chrono_tz::Tz;

/// A validated transit agency (`agency.txt`).
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agency_from_raw_and_rkyv_roundtrip() {
        let raw = RawAgency {
            id: Some(AgencyId::new("")),
            name: "RTD Denver".to_owned(),
            url: "https://www.rtd-denver.com".to_owned(),
            timezone: Tz::America__Denver,
            lang: Some("en".to_owned()),
            phone: None,
            fare_url: None,
            email: None,
        };
        let agency = Agency::from(raw);
        assert!(
            agency.id.is_none(),
            "empty agency_id should normalize to None"
        );
        assert_eq!(agency.time_zone(), Tz::America__Denver);

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&agency).unwrap();
        let restored: Agency = rkyv::from_bytes::<Agency, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(restored, agency);
    }
}
