use std::sync::Arc;

use crate::util::zone::ZoneLookup;

use super::GtfsFlexParams;

use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::{Edge, Vertex},
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{TraversalModel, TraversalModelError},
    },
};

pub struct GtfsFlexModel {
    pub lookup: Arc<ZoneLookup>,
    pub params: GtfsFlexParams,
}

impl GtfsFlexModel {
    pub fn new(lookup: Arc<ZoneLookup>, params: GtfsFlexParams) -> Self {
        // modify this and the struct definition if additional pre-processing
        // is required during model instantiation from query parameters.
        Self { lookup, params }
    }
}

impl TraversalModel for GtfsFlexModel {
    fn name(&self) -> String {
        "GtfsGtfsFlexTraversalModel".to_string()
    }

    fn input_features(&self) -> Vec<InputFeature> {
        todo!()
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        todo!()
    }

    fn traverse_edge(
        &self,
        _trajectory: (&Vertex, &Edge, &Vertex),
        _state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
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
