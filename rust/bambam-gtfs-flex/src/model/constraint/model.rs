use std::sync::Arc;

use routee_compass_core::model::{
    constraint::ConstraintModel,
    state::{StateModel, StateVariable},
    traversal::EdgeFrontierContext,
};

use crate::util::zone::ZoneLookup;

pub struct GtfsFlexDepartureConstraintModel {
    #[allow(dead_code)]
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
        _ctx: &EdgeFrontierContext,
        _state: &[StateVariable],
        _state_model: &StateModel,
    ) -> Result<bool, routee_compass_core::model::constraint::ConstraintModelError> {
        // have we transitioned onto this travel mode yet?
        // if not, we are boarding GTFS-Flex. check if the ZoneGraph would
        // consider this a valid departure.
        todo!()
    }

    fn valid_edge(
        &self,
        _edge: &routee_compass_core::model::network::Edge,
    ) -> Result<bool, routee_compass_core::model::constraint::ConstraintModelError> {
        Ok(true)
    }
}
