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
use uom::{si::f64::Time, ConstZero};

/// assigns time delays for trips that have a delay from the start of their trip.
/// for within-trip delays assigned to beginning travel in a mode, use a delay
/// during mode switch instead (doesn't exist yet)
pub struct TripArrivalDelayModel(Arc<TimeDelayLookup>);

impl TripArrivalDelayModel {
    pub fn new(lookup: Arc<TimeDelayLookup>) -> TripArrivalDelayModel {
        TripArrivalDelayModel(lookup)
    }
}

impl TraversalModelService for TripArrivalDelayModel {
    fn build(
        &self,
        query: &serde_json::Value,
    ) -> Result<Arc<dyn TraversalModel>, TraversalModelError> {
        let model: Arc<dyn TraversalModel> = Arc::new(Self::new(self.0.clone()));
        Ok(model)
    }
}

impl TraversalModel for TripArrivalDelayModel {
    fn name(&self) -> String {
        "Trip Arrival Delay Traversal Model".to_string()
    }

    fn input_features(&self) -> Vec<InputFeature> {
        vec![]
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        vec![(
            bambam_state::TRIP_ARRIVAL_DELAY.to_string(),
            StateVariableConfig::Time {
                initial: Time::ZERO,
                output_unit: Some(self.0.config.time_unit),
                accumulator: false,
            },
        )]
    }

    fn traverse_edge(
        &self,
        trajectory: (&Vertex, &Edge, &Vertex),
        state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        let (_, _, destination) = trajectory;
        add_delay_time(destination, state, state_model, self.0.clone())
    }

    fn estimate_traversal(
        &self,
        od: (&Vertex, &Vertex),
        state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        let (_, destination) = od;
        add_delay_time(destination, state, state_model, self.0.clone())
    }
}

/// at the end of each edge, write down the arrival delay to use if this location is treated as a destination
fn add_delay_time(
    destination: &Vertex,
    state: &mut Vec<StateVariable>,
    state_model: &StateModel,
    lookup: Arc<TimeDelayLookup>,
) -> Result<(), TraversalModelError> {
    if let Some(delay) = lookup.get_delay_for_vertex(destination) {
        state_model.set_time(state, bambam_state::TRIP_ARRIVAL_DELAY, &delay)?;
    }
    Ok(())
}
