//! Fare, payment, transfer limit, and ticketing enumerations (`PaymentMethod`, `Transfers`, `FareMediaType`, `DefaultFareCategory`, `TicketingType`).

use crate::enums::serialize_i16_as_str;
use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

/// Defines where a FareAttribute can be paid
#[derive(
    Debug,
    Deserialize,
    Serialize,
    Copy,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum PaymentMethod {
    /// Fare is paid on board
    #[serde(rename = "0")]
    Aboard,
    /// Fare must be paid before boarding
    #[serde(rename = "1")]
    PreBoarding,
}

/// Defines how many transfers can be done with on FareAttribute
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum Transfers {
    /// Unlimited transfers are permitted
    #[default]
    Unlimited,
    /// No transfers permitted on this fare
    NoTransfer,
    /// Riders may transfer once
    UniqueTransfer,
    /// Riders may transfer twice
    TwoTransfers,
    /// Other transfer values
    Other(i16),
}

impl<'de> Deserialize<'de> for Transfers {
    fn deserialize<D>(deserializer: D) -> Result<Transfers, D::Error>
    where
        D: Deserializer<'de>,
    {
        let i = Option::<i16>::deserialize(deserializer)?;
        Ok(match i {
            Some(0) => Transfers::NoTransfer,
            Some(1) => Transfers::UniqueTransfer,
            Some(2) => Transfers::TwoTransfers,
            Some(a) => Transfers::Other(a),
            None => Transfers::default(),
        })
    }
}

impl Serialize for Transfers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Transfers::NoTransfer => serialize_i16_as_str(serializer, 0),
            Transfers::UniqueTransfer => serialize_i16_as_str(serializer, 1),
            Transfers::TwoTransfers => serialize_i16_as_str(serializer, 2),
            Transfers::Other(a) => serialize_i16_as_str(serializer, *a),
            Transfers::Unlimited => serializer.serialize_none(),
        }
    }
}

/// Defines the type of a FareMedia
#[derive(
    Debug,
    Deserialize,
    Serialize,
    Copy,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum FareMediaType {
    /// Used when there is no fare media involved in purchasing or validating a fare product
    #[serde(rename = "0")]
    None,
    /// Physical paper ticket
    #[serde(rename = "1")]
    PhysicalPaperTicket,
    /// Physical transit card
    #[serde(rename = "2")]
    PhysicalTransitCard,
    /// cEMV (contactless Europay, Mastercard and Visa)
    #[serde(rename = "3")]
    CEmv,
    /// Mobile app
    #[serde(rename = "4")]
    MobileApp,
}

/// Specifies if an entry in rider_categories.txt should be considered the default category
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
pub enum DefaultFareCategory {
    /// Category is not considered the default.
    NotDefault = 0,
    /// Category is considered the default one.
    Default = 1,
}

impl Serialize for DefaultFareCategory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_i16_as_str(
            serializer,
            match self {
                DefaultFareCategory::NotDefault => 0,
                DefaultFareCategory::Default => 1,
            },
        )
    }
}

impl<'de> Deserialize<'de> for DefaultFareCategory {
    fn deserialize<D>(deserializer: D) -> Result<DefaultFareCategory, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = std::borrow::Cow::<str>::deserialize(deserializer)?;
        Ok(match s.as_ref() {
            "" | "0" => DefaultFareCategory::NotDefault,
            "1" => DefaultFareCategory::Default,
            &_ => {
                return Err(serde::de::Error::custom(format!(
                    "Invalid value `{s}`, expected 0 or 1"
                )));
            }
        })
    }
}

/// Specifies whether tickets can be bought for this item
#[derive(
    Debug,
    Default,
    Deserialize,
    Serialize,
    Copy,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum TicketingType {
    /// If a ticketing_deep_link_id is set, tickets are available
    #[default]
    #[serde(rename = "0")]
    Available,
    /// Tickets are unavailable
    #[serde(rename = "1")]
    Unavailable,
}
