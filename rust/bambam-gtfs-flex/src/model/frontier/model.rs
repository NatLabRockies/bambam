use std::sync::Arc;

use routee_compass_core::model::frontier::FrontierModel;

use crate::util::zone::ZoneLookup;

pub struct GtfsFlexDepartureFrontierModel {
    lookup: Arc<ZoneLookup>,
}

impl GtfsFlexDepartureFrontierModel {
    pub fn new(lookup: Arc<ZoneLookup>) -> Self {
        Self { lookup }
    }
}

impl FrontierModel for GtfsFlexDepartureFrontierModel {
    fn valid_frontier(
        &self,
        _edge: &routee_compass_core::model::network::Edge,
        _previous_edge: Option<&routee_compass_core::model::network::Edge>,
        _state: &[routee_compass_core::model::state::StateVariable],
        _state_model: &routee_compass_core::model::state::StateModel,
    ) -> Result<bool, routee_compass_core::model::frontier::FrontierModelError> {
        // have we transitioned onto this travel mode yet?
        // if not, we are boarding GTFS-Flex. check if the ZoneGraph would
        // consider this a valid departure.
        todo!()
    }

    fn valid_edge(
        &self,
        _edge: &routee_compass_core::model::network::Edge,
    ) -> Result<bool, routee_compass_core::model::frontier::FrontierModelError> {
        Ok(true)
    }
}
