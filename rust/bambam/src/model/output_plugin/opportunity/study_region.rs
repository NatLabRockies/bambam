use bamcensus_core::model::identifier::{Geoid, StateCode};
use serde::{Deserialize, Serialize};

/// Describes what study region bounds the activity dataset, as either
/// the entire nation, or, as a combination of census geoids.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum StudyRegion {
    National,
    Census { geoids: Vec<Geoid> },
}

impl StudyRegion {
    /// get the collection of [`Geoid`]s that describe this study region.
    pub fn get_geoids(&self) -> Result<Vec<Geoid>, String> {
        match self {
            StudyRegion::National => Ok(StateCode::all_state_geoids()),
            StudyRegion::Census { geoids } => Ok(geoids.clone()),
        }
    }
}
