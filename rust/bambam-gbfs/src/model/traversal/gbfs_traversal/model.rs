use std::sync::Arc;

use super::{GbfsTraversalEngine, GbfsTraversalParams};

use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::Vertex,
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{EdgeFrontierContext, TraversalModel, TraversalModelError},
    },
};

pub struct GbfsTraversalModel {
    pub engine: Arc<GbfsTraversalEngine>,
    pub params: GbfsTraversalParams,
}

impl GbfsTraversalModel {
    pub fn new(engine: Arc<GbfsTraversalEngine>, params: GbfsTraversalParams) -> Self {
        // modify this and the struct definition if additional pre-processing
        // is required during model instantiation from query parameters.
        Self { engine, params }
    }
}

impl TraversalModel for GbfsTraversalModel {
    fn name(&self) -> String {
        "GbfsTraversalModel".to_string()
    }

    fn input_features(&self) -> Vec<InputFeature> {
        todo!()
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        todo!()
    }

    fn traverse_edge(
        &self,
        _ctx: &EdgeFrontierContext,
        _state: &mut Vec<StateVariable>,
        _state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        todo!()
    }

    fn estimate_traversal(
        &self,
        _od: (&Vertex, &Vertex),
        _state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        _state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        todo!()
    }
}
