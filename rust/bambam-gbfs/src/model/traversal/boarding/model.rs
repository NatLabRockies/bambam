use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::Vertex,
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{EdgeTraversalContext, TraversalModel, TraversalModelError},
    },
};

/// applies wait times when boarding a micromobility vehicle.
pub struct BoardingTraversalModel {}

impl TraversalModel for BoardingTraversalModel {
    fn name(&self) -> String {
        "BoardingTraversalModel".to_string()
    }

    fn input_features(&self) -> Vec<InputFeature> {
        todo!()
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        todo!()
    }

    fn estimate_traversal(
        &self,
        _od: (&Vertex, &Vertex),
        _state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        _state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        // this can be skipped if we aren't trying to use A*.
        Ok(())
    }

    fn traverse_edge(
        &self,
        _ctx: &EdgeTraversalContext,
        _state: &mut Vec<StateVariable>,
        _state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        todo!()
    }
}
