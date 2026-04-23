use std::sync::Arc;

use crate::{
    model::{feature, ops, GtfsFlexParams},
    util::zone::{ZoneId, ZoneLookup},
};

use bambam_core::model::state::CategoricalMapping;
use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::Vertex,
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{EdgeFrontierContext, TraversalModel, TraversalModelError},
    },
};

pub struct GtfsFlexModel {
    pub lookup: Arc<ZoneLookup>,
    pub mapping: Arc<CategoricalMapping<ZoneId, i64>>,
    pub params: GtfsFlexParams,
}

impl GtfsFlexModel {
    pub fn new(
        lookup: Arc<ZoneLookup>,
        mapping: Arc<CategoricalMapping<ZoneId, i64>>,
        params: GtfsFlexParams,
    ) -> Self {
        Self {
            lookup,
            mapping,
            params,
        }
    }
}

impl TraversalModel for GtfsFlexModel {
    fn name(&self) -> String {
        "GtfsFlexTraversalModel".to_string()
    }

    fn input_features(&self) -> Vec<InputFeature> {
        vec![]
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        vec![
            (
                feature::fieldname::LEG_SRC_ZONE_ID.to_string(),
                feature::variable::zone_id(),
            ),
            (
                feature::fieldname::EDGE_IS_GTFS_FLEX_DESTINATION.to_string(),
                feature::variable::gtfs_flex_destination(),
            ),
            (
                feature::fieldname::EDGE_POOLING_DELAY.to_string(),
                feature::variable::pooling_delay(),
            ),
        ]
    }

    fn traverse_edge(
        &self,
        ctx: &EdgeFrontierContext,
        state: &mut Vec<StateVariable>,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        // determine if we need to inject a source zone id into the state vector.
        let not_existing_gtfs_flex_trip = no_existing_trip(state, state_model)?;
        if not_existing_gtfs_flex_trip {
            set_src_zone_id(state, state_model, ctx.dst, self)?;
        }

        // todo: pooling delay?

        // for every edge, assign whether it is a valid GTFS-Flex destination
        validate_flex_destination(state, state_model, ctx.dst, self)
    }

    fn estimate_traversal(
        &self,
        _od: (&Vertex, &Vertex),
        _state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        _state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        Ok(())
    }
}

fn no_existing_trip(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<bool, TraversalModelError> {
    let is_set = ops::src_zone_id_set(state, state_model).map_err(|e| {
        TraversalModelError::TraversalModelFailure(format!("while checking for existing trip, {e}"))
    })?;
    Ok(!is_set)
}

/// helper function to look up the ZoneId of this destination vertex and add it to the trip state.
fn set_src_zone_id(
    state: &mut Vec<StateVariable>,
    state_model: &StateModel,
    dst: &Vertex,
    model: &GtfsFlexModel,
) -> Result<(), TraversalModelError> {
    let zone_id_opt = model.lookup.get_zone_for_vertex(dst).map_err(|e| {
        let msg = format!("while getting zone for vertex {dst:?}, {e}");
        TraversalModelError::InternalError(msg)
    })?;
    match zone_id_opt {
        Some(src_zone_id) => {
            log::debug!("gtfs-flex traversal boarding at zone {src_zone_id}");
            ops::set_src_zone_id(&src_zone_id, state, state_model, &model.mapping).map_err(|e| {
                let msg =
                    format!("while assigning src_zone_id for vertex {dst:?} in state vector, {e}");
                TraversalModelError::TraversalModelFailure(msg)
            })
        }
        None => {
            let msg = format!("during GTFS-Flex traversal, entered edge with '{}' unset and no intersecting zonal data. should be unreachable, prevented by the constraint model.",
                feature::fieldname::LEG_SRC_ZONE_ID
            );
            Err(TraversalModelError::InternalError(msg))
        }
    }
}

/// Validates whether the destination vertex is a valid GTFS-Flex stop from the source zone
/// at the current time. Updates the state with the validity result.
///
/// Assumes a source zone has already been set on the state from a previous leg of the trip.
fn validate_flex_destination(
    state: &mut Vec<StateVariable>,
    state_model: &StateModel,
    dst: &Vertex,
    model: &GtfsFlexModel,
) -> Result<(), TraversalModelError> {
    // find out if we can label this as a valid destination
    let src_zone_id = ops::get_src_zone_id(state, state_model, &model.mapping)
        .map_err(|e| {
            let msg = format!("while validating flex destination, {e}");
            TraversalModelError::TraversalModelFailure(msg)
        })?
        .ok_or_else(|| {
            let msg = format!(
                "field '{}' exists in state get_src_zone_id fails",
                feature::fieldname::LEG_SRC_ZONE_ID
            );
            TraversalModelError::InternalError(msg)
        })?;

    let current_datetime =
        ops::create_current_datetime(&model.params.start_time, state, state_model).map_err(
            |e| {
                let msg = format!("while validating flex destination, {e}");
                TraversalModelError::TraversalModelFailure(msg)
            },
        )?;

    let is_valid = model
        .lookup
        .valid_destination(src_zone_id, dst, &current_datetime)?;
    log::debug!(
        "gtfs-flex traversal reaches vertex {} ({},{}) at time {}. is a valid destination? {is_valid}",
        dst.vertex_id,
        dst.x(),
        dst.y(),
        current_datetime.format("%Y-%m-%d %H:%M:%S")
    );
    ops::set_is_valid(is_valid, state, state_model).map_err(|e| {
        let msg =
            format!("while assigning valid destination for vertex {dst:?} in state vector, {e}");
        TraversalModelError::TraversalModelFailure(msg)
    })
}
