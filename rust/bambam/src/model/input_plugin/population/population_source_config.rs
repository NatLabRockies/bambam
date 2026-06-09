use crate::model::input_plugin::population::{
    population_source::PopulationSource, us_states_lookup,
};
use bamcensus_acs::model::AcsType;
use bamcensus_core::model::identifier::GeoidType;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PopulationSourceConfig {
    #[serde(rename = "acs")]
    UsCensusAcs {
        acs_type: AcsType,
        acs_year: u64,
        acs_resolution: Option<GeoidType>,
        acs_categories: Option<Vec<String>>,
        api_token: String,
    },
}

impl PopulationSourceConfig {
    pub fn build(&self) -> Result<PopulationSource, String> {
        match self {
            PopulationSourceConfig::UsCensusAcs {
                acs_type,
                acs_year,
                acs_resolution,
                acs_categories,
                api_token,
            } => {
                let states = us_states_lookup::load()?;
                let source = PopulationSource::UsCensusAcs {
                    states,
                    acs_type: *acs_type,
                    acs_year: *acs_year,
                    acs_resolution: *acs_resolution,
                    acs_categories: acs_categories.clone(),
                    api_token: api_token.clone(),
                };
                Ok(source)
            }
        }
    }
}
