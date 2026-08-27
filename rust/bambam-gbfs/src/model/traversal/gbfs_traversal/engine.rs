use std::path::Path;

use crate::model::{
    feature,
    gbfs::{GbfsLookupModel, ZoneState},
};

use super::GbfsTraversalConfig;

use bambam_core::model::state::CategoricalStateMapping;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use routee_compass_core::model::{
    constraint::ConstraintModelError,
    network::Vertex,
    state::{StateModel, StateVariable},
    traversal::TraversalModelError,
};

pub struct GbfsTraversalEngine {
    lookup: GbfsLookupModel,
    mapping: CategoricalStateMapping,
    pub default_speed: uom::si::f64::Velocity,
}

impl GbfsTraversalEngine {
    /// runs the traversal logic for the GBFS traversal model at an iteration of graph search.
    pub fn traverse(
        &self,
        vertex: &Vertex,
        state: &mut [StateVariable],
        state_model: &StateModel,
        start_time: DateTime<Utc>,
    ) -> Result<(), TraversalModelError> {
        // are we currently boarded with a system_id?
        let service_opt = feature::state::get_system_id(state, state_model, &self.mapping)
            .map_err(|e| {
                let msg = format!("failure inspecting gbfs service id of search state: {e}");
                TraversalModelError::TraversalModelFailure(msg)
            })?;

        // find intersecting zones. if we are already boarded, only accept zones with our existing system_id.
        let zones = self
            .lookup
            .matching_zones(vertex, state, state_model, start_time, service_opt)
            .map_err(|e| {
                let vertex_id = vertex.vertex_id;
                let service_msg = match service_opt {
                    Some(id) => format!("for trip boarded on service '{id}'"),
                    None => "for unboarded trip".to_string(),
                };
                let msg = format!(
                    "failure getting gbfs zone rules for vertex {vertex_id} {service_msg}: {e}"
                );
                TraversalModelError::TraversalModelFailure(msg)
            })?;

        match service_opt {
            Some(_) => process_boarded(&zones, self.default_speed, state, state_model),
            None => process_unboarded(zones, self.default_speed, state, state_model, &self.mapping),
        }
    }
}

impl TryFrom<GbfsTraversalConfig> for GbfsTraversalEngine {
    type Error = ConstraintModelError;

    fn try_from(config: GbfsTraversalConfig) -> Result<Self, Self::Error> {
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

        let default_speed = config
            .default_speed
            .speed_unit
            .to_uom(config.default_speed.speed);
        Ok(Self {
            lookup,
            mapping,
            default_speed,
        })
    }
}

/// traversal is associated with a trip that has already boarded GBFS. here we must
//   1. determine if this location is a valid destination, and set if true (ride_end_allowed)
//   2. write edge_speed (either from config value or overriding value from record)
//   3. (todo) if station_parking -> confirm we are at a station location
fn process_boarded(
    zones: &[ZoneState],
    default_speed: uom::si::f64::Velocity,
    state: &mut [StateVariable],
    state_model: &StateModel,
) -> Result<(), TraversalModelError> {
    use uom::si::{f64::Velocity, velocity::kilometer_per_hour};
    let zone = match zones {
        [zone] => zone,
        _ => {
            let n_zones = zones.len();
            let msg = format!(
                "expected a boarded GBFS trip would match exactly one zone state, found {n_zones}."
            );
            return Err(TraversalModelError::TraversalModelFailure(msg));
        }
    };

    // 1. is this a valid destination? by default, we assume yes, but here
    // we overwrite as false if `ride_end_allowed` is false.
    if !zone.ride_end_allowed {
        feature::state::set_invalid_destination(state, state_model)?;
    }
    // 2. set the speed, limiting if max speed is present
    let speed_value: Velocity = match zone.maximum_speed_kph {
        Some(max) => {
            let max_vel = Velocity::new::<kilometer_per_hour>(max.into());
            max_vel.min(default_speed)
        }
        None => default_speed,
    };
    state_model.set_speed(state, "edge_speed", &speed_value)?;

    Ok(())
}

/// traversal is associated with a trip that has not yet boarded GBFS. here we must
///   1. pick the best-quality system from the intersecting zones
///     - ride_start_allowed is true
///     - best speed
///     - sort ids lexicagraphically
///   2. board with that system_id
///   3. process boarded
fn process_unboarded(
    zones: Vec<ZoneState>,
    default_speed: uom::si::f64::Velocity,
    state: &mut [StateVariable],
    state_model: &StateModel,
    mapping: &CategoricalStateMapping,
) -> Result<(), TraversalModelError> {
    let n_zones = zones.len();

    let best_zone = zones
        .into_iter()
        .filter(|z| z.ride_start_allowed && z.ride_through_allowed)
        .sorted_by_cached_key(|z| z.ascending_sort_key())
        .next();

    match best_zone {
        Some(best) => {
            feature::state::set_system_id(state, state_model, &best.system_id, mapping)?;
            process_boarded(&[best], default_speed, state, state_model)
        }
        None => {
            let msg = format!(
                "found {n_zones} zones but none are ride_start_allowed; should have been caught by constraint model!",
            );
            Err(TraversalModelError::InternalError(msg))
        }
    }
}
