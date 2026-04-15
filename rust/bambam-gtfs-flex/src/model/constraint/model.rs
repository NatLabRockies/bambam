use std::sync::Arc;

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
};

use crate::model::ops;
use crate::{model::feature, util::zone::ZoneLookup};

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
        _edge: &Edge,
        _previous_edge: Option<&Edge>,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        let departing_gtfs_flex_trip = ops::src_zone_id_unset(state, state_model)
            .map_err(|e| ConstraintModelError::ConstraintModelError(e.to_string()))?;
        if departing_gtfs_flex_trip {
            // TODO: gotcha! we don't have a Vertex in scope :-(
            // let this_zone_id = self.lookup.get_zone_for_vertex(vertex)
            // self.lookup.valid_departure(src_zone_id, current_time)
        }

        // pending implementation logic:
        //   - we have not started our trip and this edge is in a zone -> ACCEPT!
        //   - we have started our trip -> ACCEPT!
        //   - we have not started our trip and this edge is not in a zone -> REJECT!

        todo!()
    }

    fn valid_edge(&self, _edge: &Edge) -> Result<bool, ConstraintModelError> {
        Ok(true)
    }
}
