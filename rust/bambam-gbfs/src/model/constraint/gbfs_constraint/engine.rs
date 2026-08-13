use std::path::Path;

use crate::model::{
    feature,
    gbfs::{GbfsLookupModel, ops},
};

use super::GbfsConstraintConfig;

use bambam_core::model::state::CategoricalStateMapping;
use chrono::{DateTime, Utc};
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Vertex,
    state::{StateModel, StateVariable},
};

pub struct GbfsConstraintEngine {
    lookup: GbfsLookupModel,
    mapping: CategoricalStateMapping,
}

impl GbfsConstraintEngine {
    /// tests:
    ///   - are we NOT in a zone? FALSE
    ///   - have we boarded?
    ///     - if FALSE, also check if `ride_start_allowed`
    ///   - does that zone support `ride_through_allowed`? TRUE
    ///   - otherwise FALSE
    pub fn check_valid(
        &self,
        vertex: &Vertex,
        state: &[StateVariable],
        state_model: &StateModel,
        start_time: DateTime<Utc>,
    ) -> Result<bool, ConstraintModelError> {
        let service_opt = feature::state::get_system_id(state, state_model, &self.mapping)
            .map_err(|e| {
                let msg = format!("failure inspecting service id of search state: {e}");
                ConstraintModelError::ConstraintModelError(msg)
            })?;
        let current_time = ops::current_datetime(start_time, state, state_model).map_err(|e| {
            let msg: String = format!("failure running GBFS constraint model: {e}");
            ConstraintModelError::ConstraintModelError(msg)
        })?;
        let zones = self
            .lookup
            .matching_zones(vertex, state, state_model, current_time, service_opt)
            .map_err(|e| {
                let msg = format!("failure running GBFS rule lookup: {e}");
                ConstraintModelError::ConstraintModelError(msg)
            })?;

        let not_in_a_zone = zones.is_empty();
        let through_allowed = zones.iter().any(|z| z.ride_through_allowed);
        let no_depart_allowed =
            service_opt.is_none() && !zones.iter().any(|z| z.ride_start_allowed);

        if not_in_a_zone || no_depart_allowed {
            Ok(false)
        } else {
            Ok(through_allowed)
        }
    }
}

impl TryFrom<GbfsConstraintConfig> for GbfsConstraintEngine {
    type Error = ConstraintModelError;

    fn try_from(config: GbfsConstraintConfig) -> Result<Self, Self::Error> {
        let lookup = GbfsLookupModel::new(
            &config.zone_record_input_file,
            &config.zone_geometry_input_file,
        )
        .map_err(|e| {
            let msg = format!("failure building GBFS lookup model: {e}");
            ConstraintModelError::BuildError(msg)
        })?;
        let mapping = CategoricalStateMapping::from_enumerated_category_file(Path::new(
            &config.system_ids_input_file,
        ))
        .map_err(|e| {
            ConstraintModelError::BuildError(format!(
                "failure while building categorical mapping from {}: {e}",
                config.system_ids_input_file
            ))
        })?;
        Ok(Self { lookup, mapping })
    }
}
