use std::sync::Arc;

use crate::{
    model::{feature, ops},
    util::zone::{ZoneId, ZoneLookup},
};

use super::GtfsFlexParams;

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
        let existing_gtfs_flex_trip = !ops::src_zone_id_set(state, state_model)?;
        if existing_gtfs_flex_trip {
            inject_src_zone_id(state, state_model, ctx.dst, self)?;
        }

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

fn inject_src_zone_id(
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
        Some(src_zone_id) => ops::set_src_zone_id(&src_zone_id, state, state_model, &model.mapping),
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
    let src_zone_id =
        ops::get_src_zone_id(state, state_model, &model.mapping)?.ok_or_else(|| {
            let msg = format!(
                "field '{}' exists in state get_src_zone_id fails",
                feature::fieldname::LEG_SRC_ZONE_ID
            );
            TraversalModelError::InternalError(msg)
        })?;
    let current_datetime =
        ops::create_current_datetime(&model.params.start_time, state, state_model)?;
    let is_valid = model
        .lookup
        .valid_destination(src_zone_id, dst, &current_datetime)?;
    ops::set_is_valid(is_valid, state, state_model)
}
