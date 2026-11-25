use std::{collections::HashMap, sync::Arc};

use crate::traversal::lts::{
    config::LtsMappingConfig,
    engine::{LtsMappingOps, LtsOp},
};

use super::{LtsEngine, LtsParams};

use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        network::Vertex,
        state::{InputFeature, StateModel, StateVariable, StateVariableConfig},
        traversal::{
            default::fieldname::EDGE_SPEED, EdgeFrontierContext, TraversalModel,
            TraversalModelError,
        },
    },
};
use uom::{si::f64::Velocity, ConstZero};

pub struct LtsModel {
    pub engine: Arc<LtsEngine>,
    pub params: LtsParams,
    pub mapping: LtsMappingOps,
}

impl LtsModel {
    pub const EDGE_LTS_SPEED: &'static str = "edge_lst_speed";

    pub fn new(engine: Arc<LtsEngine>, params: LtsParams) -> Result<Self, TraversalModelError> {
        // modify this and the struct definition if additional pre-processing
        // is required during model instantiation from query parameters.
        let mapping = compile_mapping(engine.config.mapping.as_ref(), params.mapping.as_ref())?;
        Ok(Self {
            engine,
            params,
            mapping,
        })
    }
}

impl TraversalModel for LtsModel {
    fn name(&self) -> String {
        "LtsModel".to_string()
    }

    fn input_features(&self) -> Vec<InputFeature> {
        vec![InputFeature::Speed {
            name: EDGE_SPEED.to_string(),
            unit: None,
        }]
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        vec![(
            Self::EDGE_LTS_SPEED.to_string(),
            StateVariableConfig::Speed {
                initial: Velocity::ZERO,
                accumulator: false,
                output_unit: Some(self.engine.config.speed_unit),
            },
        )]
    }

    fn traverse_edge(
        &self,
        ctx: &EdgeFrontierContext,
        state: &mut Vec<StateVariable>,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        let edge_speed = state_model.get_speed(state, EDGE_SPEED)?;
        let lts_speed = self
            .engine
            .get_lts_speed(ctx.edge.edge_id, edge_speed, &self.mapping)?;
        state_model.set_speed(state, Self::EDGE_LTS_SPEED, &lts_speed)?;
        Ok(())
    }

    fn estimate_traversal(
        &self,
        _od: (&Vertex, &Vertex),
        state: &mut Vec<StateVariable>,
        _tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        // since an artifical edge will not have an LTS value in our dataset, we assume
        // no loss in speed performance.
        let edge_speed = state_model.get_speed(state, EDGE_SPEED)?;
        state_model.set_speed(state, Self::EDGE_LTS_SPEED, &edge_speed)?;
        Ok(())
    }
}

/// compiles the configured mapping, preferring a parameter-time
/// override over a configuration-time default.
fn compile_mapping(
    config: Option<&LtsMappingConfig>,
    params: Option<&LtsMappingConfig>,
) -> Result<LtsMappingOps, TraversalModelError> {
    let chosen = match (config, params) {
        (None, None) => {
            return Err(TraversalModelError::BuildError(
                "no LTS mapping provided (config, params)".to_string(),
            ))
        }
        (None, Some(p)) => p,
        (Some(c), None) => c,
        (Some(_), Some(p)) => p,
    };
    parse_mapping(chosen)
}

fn parse_mapping(mapping: &LtsMappingConfig) -> Result<LtsMappingOps, TraversalModelError> {
    let mut result = HashMap::with_capacity(mapping.0.len());
    for (k, v) in mapping.0.iter() {
        let op: LtsOp = v.try_into()?;
        result.insert(*k, op);
    }
    Ok(result)
}
