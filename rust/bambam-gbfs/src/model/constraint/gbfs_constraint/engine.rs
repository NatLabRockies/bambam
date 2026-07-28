use crate::model::gbfs::{GbfsLookupModel, GbfsZoneRecord};

use super::GbfsConstraintConfig;

use chrono::{DateTime, Utc};
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Vertex,
    state::{StateModel, StateVariable},
};

pub struct GbfsConstraintEngine {
    config: GbfsConstraintConfig,
    lookup: GbfsLookupModel,
}

impl GbfsConstraintEngine {
    /// tests:
    ///   - are we NOT in a zone? FALSE
    ///   - does that zone support `ride_through_allowed`? TRUE
    ///   - otherwise FALSE
    fn check_valid(
        &self,
        vertex: &Vertex,
        state: &[StateVariable],
        state_model: &StateModel,
        start_time: DateTime<Utc>,
    ) -> Result<bool, String> {
        let zone = self
            .lookup
            .get_zone_rules(vertex, state, state_model, start_time)?;
        match zone {
            None => Ok(false), // no zones intersect, this is not a valid place to GBFS
            Some(z) => Ok(z.ride_through_allowed),
        }
    }
}

impl TryFrom<GbfsConstraintConfig> for GbfsConstraintEngine {
    type Error = ConstraintModelError;

    fn try_from(config: GbfsConstraintConfig) -> Result<Self, Self::Error> {
        let lookup = GbfsLookupModel::try_from(&config).map_err(|e| {
            let msg = format!("failure building GBFS lookup model: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        Ok(Self { config, lookup })
    }
}
