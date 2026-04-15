use std::sync::Arc;

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
    traversal::EdgeFrontierContext,
};

use crate::model::ops;
use crate::util::zone::ZoneLookup;

pub struct GtfsFlexDepartureConstraintModel {
    lookup: Arc<ZoneLookup>,
}

impl GtfsFlexDepartureConstraintModel {
    pub fn new(lookup: Arc<ZoneLookup>) -> Self {
        Self { lookup }
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
        let existing_gtfs_flex_trip = ops::src_zone_id_set(state, state_model)
            .map_err(|e| ConstraintModelError::ConstraintModelError(e.to_string()))?;
        if existing_gtfs_flex_trip {
            return Ok(true);
        }

        // GTFS-Flex trip has not yet started. reject this edge if it is not in a zone,
        // otherwise accept as we will board here.
        let this_zone_id = self
            .lookup
            .get_zone_for_vertex(ctx.dst)
            .map_err(|e| ConstraintModelError::ConstraintModelError(e.to_string()))?;
        Ok(this_zone_id.is_some())
    }

    fn valid_edge(&self, _edge: &Edge) -> Result<bool, ConstraintModelError> {
        Ok(true)
    }
}
