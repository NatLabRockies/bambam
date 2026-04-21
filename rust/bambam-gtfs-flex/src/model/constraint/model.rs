use std::sync::Arc;

use bambam_core::model::state::multimodal_state_ops;
use chrono::NaiveDateTime;
use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
    traversal::EdgeFrontierContext,
};

use crate::util::zone::ZoneLookup;
use crate::{
    model::{ops, GtfsFlexParams},
    util::zone::ZoneId,
};

pub struct GtfsFlexDepartureConstraintModel {
    lookup: Arc<ZoneLookup>,
    params: GtfsFlexParams,
}

impl GtfsFlexDepartureConstraintModel {
    pub fn new(lookup: Arc<ZoneLookup>, params: GtfsFlexParams) -> Self {
        Self { lookup, params }
    }
}

impl ConstraintModel for GtfsFlexDepartureConstraintModel {
    fn valid_frontier(
        &self,
        ctx: &EdgeFrontierContext,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        // if we have started our trip already, any edge is acceptable.
        let existing_gtfs_flex_trip = existing_trip(state, state_model)?;
        if existing_gtfs_flex_trip {
            log::debug!(
                "gtfs-flex frontier is valid? yes, this is an already-existing trip, {}",
                log_context(ctx, state, state_model)
            );
            return Ok(true);
        }

        // GTFS-Flex trip has not yet started. reject this edge if it is not in a zone or if it
        // there is no supporting relation in the ZoneGraph, otherwise, accept as we will board here.
        let current_time = current_datetime(self.params.start_time, state, state_model)?;
        let lookup_result = self
            .lookup
            .get_zone_for_vertex(ctx.dst)
            .map_err(|e| ConstraintModelError::ConstraintModelError(e.to_string()))?;
        let is_valid = match lookup_result {
            Some(src_zone_id) => is_valid_departure(&self.lookup, &src_zone_id, &current_time),
            None => Ok(false),
        }?;
        log::debug!(
            "gtfs-flex frontier is valid (can board here)? {is_valid}, {}",
            log_context(ctx, state, state_model)
        );
        Ok(is_valid)
    }

    fn valid_edge(&self, _edge: &Edge) -> Result<bool, ConstraintModelError> {
        Ok(true)
    }
}

/// helper to check if a gtfs-flex trip is already active on the state vector.
fn existing_trip(
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<bool, ConstraintModelError> {
    let is_set = ops::src_zone_id_set(state, state_model).map_err(|e| {
        let msg = format!(
            "while validating frontier for gtfs-flex trip and testing if src_zone_id is set, {e}"
        );
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    Ok(is_set)
}

fn current_datetime(
    start_time: NaiveDateTime,
    state: &[StateVariable],
    state_model: &StateModel,
) -> Result<NaiveDateTime, ConstraintModelError> {
    ops::create_current_datetime(&start_time, state, state_model)
        .map_err(|e| {
            let msg = format!("while validating frontier for gtfs-flex trip and constructing current datetime at edge traversal, {e}");
            ConstraintModelError::ConstraintModelError(msg)
        })
}

fn is_valid_departure(
    lookup: &ZoneLookup,
    src_zone_id: &ZoneId,
    current_time: &NaiveDateTime,
) -> Result<bool, ConstraintModelError> {
    lookup
        .valid_departure(src_zone_id, current_time)
        .map_err(|e| {
            let msg =
                format!("while validating frontier for gtfs-flex trip via ZoneGraph lookup, {e}");
            ConstraintModelError::ConstraintModelError(msg)
        })
}

/// helper to display label, edge, and relevant state information into debug logs.
fn log_context(
    ctx: &EdgeFrontierContext,
    state: &[StateVariable],
    state_model: &StateModel,
) -> String {
    format!(
        "for label {:?}, edge {:?} with active_leg {}, trip_time: {:.2} minutes",
        ctx.parent_label,
        (ctx.edge.edge_list_id, ctx.edge.edge_id),
        multimodal_state_ops::get_active_leg_idx(state, state_model)
            .unwrap_or_default()
            .unwrap_or_default(),
        state_model
            .get_time(state, "trip_time")
            .unwrap_or_default()
            .get::<uom::si::time::minute>(),
    )
}
