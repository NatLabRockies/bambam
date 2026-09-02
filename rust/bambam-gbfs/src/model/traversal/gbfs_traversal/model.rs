use std::sync::Arc;

use crate::model::feature;

use super::{GbfsTraversalEngine, GbfsTraversalParams};

use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::Vertex,
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{EdgeFrontierContext, TraversalModel, TraversalModelError},
    },
};
use uom::si::f64::Velocity;

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
        vec![]
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        // 1. valid destination
        // 2. max_speed + edge_speed
        vec![
            (
                feature::fieldname::GBFS_DESTINATION.to_string(),
                feature::variable::gbfs_destination(),
            ),
            (
                feature::fieldname::GBFS_SYSTEM_ID.to_string(),
                feature::variable::gbfs_system_id(),
            ),
            (
                "edge_speed".to_string(),
                StateVariableConfig::Speed {
                    initial: Velocity::new::<uom::si::velocity::kilometer_per_hour>(0.0),
                    accumulator: false,
                    output_unit: None,
                },
            ),
        ]
    }

    fn traverse_edge(
        &self,
        ctx: &EdgeFrontierContext,
        state: &mut Vec<StateVariable>,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        self.engine
            .traverse(ctx.dst, state, state_model, self.params.start_time)
    }

    fn estimate_traversal(
        &self,
        _od: (&Vertex, &Vertex),
        state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        state_model.set_speed(state, "edge_speed", &self.engine.default_speed)?;
        Ok(())
    }
}
