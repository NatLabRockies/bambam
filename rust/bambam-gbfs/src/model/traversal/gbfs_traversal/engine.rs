use std::path::Path;

use crate::model::gbfs::GbfsLookupModel;

use super::GbfsTraversalConfig;

use bambam_core::model::state::CategoricalStateMapping;
use chrono::{DateTime, Utc};
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Vertex,
    state::{StateModel, StateVariable},
    traversal::TraversalModelError,
};

pub struct GbfsTraversalEngine {
    lookup: GbfsLookupModel,
    mapping: CategoricalStateMapping,
}

impl GbfsTraversalEngine {
    pub fn traverse(
        &self,
        _vertex: &Vertex,
        _state: &[StateVariable],
        _state_model: &StateModel,
        _start_time: DateTime<Utc>,
    ) -> Result<(), TraversalModelError> {
        todo!("1) what is the sequence of ops here? doc them; 2) write logic")
        // let service_opt = feature::state::get_service_id(state, state_model, &self.mapping)
        //     .map_err(|e| format!("failure inspecting service id of search state: {e}"))?;
        // let zones =
        //     self.lookup
        //         .get_zone_rules(vertex, state, state_model, start_time, service_opt)?;

        // let not_in_a_zone = zones.len() == 0;
        // let through_allowed = zones.iter().any(|z| z.ride_through_allowed);
        // let no_depart_allowed =
        //     service_opt.is_none() && !zones.iter().any(|z| z.ride_start_allowed);

        // if not_in_a_zone || no_depart_allowed {
        //     Ok(false)
        // } else {
        //     Ok(through_allowed)
        // }
    }
}

impl TryFrom<GbfsTraversalConfig> for GbfsTraversalEngine {
    type Error = ConstraintModelError;

    fn try_from(config: GbfsTraversalConfig) -> Result<Self, Self::Error> {
        let lookup = GbfsLookupModel::new(&config.zones_input_file, &config.geometries_input_file)
            .map_err(|e| {
                let msg = format!("failure building GBFS lookup model: {e}");
                ConstraintModelError::BuildError(msg)
            })?;
        let mapping = CategoricalStateMapping::from_enumerated_category_file(Path::new(
            &config.zone_ids_input_file,
        ))
        .map_err(|e| {
            ConstraintModelError::BuildError(format!(
                "failure while building categorical mapping from {}: {e}",
                config.zone_ids_input_file
            ))
        })?;
        Ok(Self { lookup, mapping })
    }
}
