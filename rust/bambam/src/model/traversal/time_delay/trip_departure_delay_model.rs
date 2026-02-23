use super::TimeDelayLookup;
use bambam_core::model::bambam_state;
use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::{Edge, Vertex},
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{TraversalModel, TraversalModelError, TraversalModelService},
    },
};
use std::sync::Arc;
use uom::{
    si::f64::{Length, Time},
    ConstZero,
};

/// assigns time delays for trips that have a delay from the start of their trip.
/// for within-trip delays assigned to beginning travel in a mode, use a delay
/// during mode switch instead (doesn't exist yet)
pub struct TripDepartureDelayModel(Arc<TimeDelayLookup>);

impl TripDepartureDelayModel {
    pub fn new(lookup: Arc<TimeDelayLookup>) -> TripDepartureDelayModel {
        TripDepartureDelayModel(lookup)
    }
}

impl TraversalModelService for TripDepartureDelayModel {
    fn build(
        &self,
        query: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let model: Arc<dyn TraversalModel> = Arc::new(Self::new(self.0.clone()));
        Ok(model)
    }
}

impl TraversalModel for TripDepartureDelayModel {
    fn name(&self) -> String {
        "Trip Departure Delay Traversal Model".to_string()
    }

    fn input_features(&self) -> Vec<InputFeature> {
        vec![]
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        vec![
            (
                bambam_state::TRIP_TIME.to_string(),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    output_unit: Some(self.0.config.time_unit),
                    accumulator: false,
                },
            ),
            (
                bambam_state::TRIP_ENROUTE_DELAY.to_string(),
                StateVariableConfig::Time {
                    initial: Time::ZERO,
                    output_unit: Some(self.0.config.time_unit),
                    accumulator: false,
                },
            ),
        ]
    }

    fn traverse_edge(
        &self,
        trajectory: (&Vertex, &Edge, &Vertex),
        state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        let (origin, _, _) = trajectory;
        add_delay_time(origin, state, state_model, self.0.clone())
    }

    fn estimate_traversal(
        &self,
        od: (&Vertex, &Vertex),
        state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        let (origin, _) = od;
        add_delay_time(origin, state, state_model, self.0.clone())
    }
}

/// if trip is departing from the origin, apply the trip departure delay.
fn add_delay_time(
    origin: &Vertex,
    state: &mut Vec<StateVariable>,
    state_model: &StateModel,
    lookup: Arc<TimeDelayLookup>,
) -> Result<(), TraversalModelError> {
    let distance = state_model.get_distance(state, bambam_state::TRIP_DISTANCE)?;
    if distance == Length::ZERO {
        return Ok(());
    }
    if let Some(delay) = lookup.get_delay_for_vertex(origin) {
        state_model.set_time(state, bambam_state::TRIP_ENROUTE_DELAY, &delay)?;
        state_model.add_time(state, bambam_state::TRIP_TIME, &delay)?;
    }
    Ok(())
}
