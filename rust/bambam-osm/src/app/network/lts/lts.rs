use crate::app::network::common::cycleway_tag::CyclewayTag;

pub const MIN_LTS: u8 = 1; // the best LTS score.
pub const MAX_LTS: u8 = 4; // the worst LTS score.

#[derive(Default, Eq, PartialEq, PartialOrd, Debug)]
pub struct Lts(u8);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LtsError {
    #[error("Lts value '{0}' must be in the integer range: [1..4]")]
    ValueError(u8),
}

impl std::fmt::Display for Lts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Lts {
    pub fn new(value: u8) -> Result<Self, LtsError> {
        if !(MIN_LTS..=MAX_LTS).contains(&value) {
            Err(LtsError::ValueError(value))
        } else {
            Ok(Lts(value))
        }
    }

    pub fn from_table_lookup(
        traffic_speed: u8, // assumed in mph.
        cycleway_tag: CyclewayTag,
        oneway_street: bool,
    ) -> Result<Lts, LtsError> {
        let value = match (cycleway_tag, oneway_street) {
            (CyclewayTag::DedicatedWithBuffer, _) => 1,

            // Dedicated cycleway without buffer
            (CyclewayTag::DedicatedNoBuffer, false) => match traffic_speed {
                0..=30 => 1,
                31..=35 => 2,
                36..=40 => 3,
                _ => 4,
            },
            (CyclewayTag::DedicatedNoBuffer, true) => match traffic_speed {
                0..=30 => 1,
                31..=40 => 2,
                _ => 3,
            },

            // No dedicated cycleway, but bike facilities present
            (CyclewayTag::NoDedicatedWithFacilities, false) => match traffic_speed {
                0..=25 => 2,
                26..=40 => 3,
                _ => 4,
            },
            (CyclewayTag::NoDedicatedWithFacilities, true) => match traffic_speed {
                0..=35 => 2,
                36..=45 => 3,
                _ => 4,
            },

            // Mixed traffic (no dedicated lane, no extra facilities)
            (CyclewayTag::NoDedicatedNoFacilities, false) => match traffic_speed {
                0..=25 => 2,
                26..=35 => 3,
                _ => 4,
            },
            (CyclewayTag::NoDedicatedNoFacilities, true) => match traffic_speed {
                0..=30 => 2,
                31..=40 => 3,
                _ => 4,
            },
        };

        Lts::new(value)
    }
}
