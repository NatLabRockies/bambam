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
    pub const EDGE_LTS_SPEED: &'static str = "edge_lts_speed";

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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;

    use routee_compass_core::{
        algorithm::search::SearchTree,
        model::{
            label::Label,
            network::{Edge, EdgeId, Vertex, VertexId},
            state::{StateModel, StateVariableConfig},
            traversal::{
                default::fieldname::EDGE_SPEED, EdgeFrontierContext, TraversalModelBuilder,
                TraversalModelError,
            },
            unit::SpeedUnit,
        },
    };
    use serde_json::json;
    use tempfile::NamedTempFile;
    use uom::{
        si::f64::{Length, Velocity},
        ConstZero,
    };

    use crate::{
        lts::lts::Lts,
        traversal::lts::{
            config::{LtsConfig, LtsMappingConfig, LtsOperationConfig},
            engine::{LtsEngine, LtsOp},
            LtsBuilder, LtsModel, LtsParams,
        },
    };

    fn write_temp_lts_file(lines: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file
    }

    fn default_mapping_config() -> LtsMappingConfig {
        let mut map = std::collections::HashMap::new();
        map.insert(
            Lts::new(1).unwrap(),
            LtsOperationConfig::PercentPenalty { percent: 100.0 },
        );
        map.insert(
            Lts::new(2).unwrap(),
            LtsOperationConfig::PercentPenalty { percent: 90.0 },
        );
        map.insert(
            Lts::new(3).unwrap(),
            LtsOperationConfig::PercentPenalty { percent: 75.0 },
        );
        map.insert(
            Lts::new(4).unwrap(),
            LtsOperationConfig::PercentPenalty { percent: 50.0 },
        );
        LtsMappingConfig(map)
    }

    #[test]
    fn test_lts_op_apply_and_bounds_validation() {
        let op = LtsOp::try_from(&LtsOperationConfig::PercentPenalty { percent: 80.0 }).unwrap();
        let base_speed = Velocity::new::<uom::si::velocity::mile_per_hour>(10.0);
        let penalized = op.apply(base_speed);
        let mph = penalized.get::<uom::si::velocity::mile_per_hour>();
        assert!((mph - 8.0).abs() < 1e-9);

        // Invalid percentages should fail
        assert!(LtsOp::try_from(&LtsOperationConfig::PercentPenalty { percent: 0.0 }).is_err());
        assert!(LtsOp::try_from(&LtsOperationConfig::PercentPenalty { percent: -10.0 }).is_err());
        assert!(LtsOp::try_from(&LtsOperationConfig::PercentPenalty { percent: 100.1 }).is_err());
        assert!(
            LtsOp::try_from(&LtsOperationConfig::PercentPenalty { percent: f64::NAN }).is_err()
        );
        assert!(LtsOp::try_from(&LtsOperationConfig::PercentPenalty {
            percent: f64::INFINITY
        })
        .is_err());
    }

    #[test]
    fn test_get_lts_speed_across_scores_and_errors() {
        let file = write_temp_lts_file(&["1", "2", "3", "4"]);
        let config = LtsConfig {
            lts_input_file: file.path().to_str().unwrap().to_string(),
            speed_unit: SpeedUnit::MPH,
            mapping: Some(default_mapping_config()),
        };
        let engine = Arc::new(LtsEngine::try_from(config).unwrap());
        let model = LtsModel::new(engine.clone(), LtsParams { mapping: None }).unwrap();

        let base_speed = Velocity::new::<uom::si::velocity::mile_per_hour>(10.0);
        let expected_mph = [10.0, 9.0, 7.5, 5.0];
        for (edge_idx, expected) in expected_mph.iter().enumerate() {
            let speed = engine
                .get_lts_speed(EdgeId(edge_idx), base_speed, &model.mapping)
                .unwrap();
            let mph = speed.get::<uom::si::velocity::mile_per_hour>();
            assert!((mph - expected).abs() < 1e-9);
        }

        // Out-of-bounds EdgeId fails
        let oob_err = engine
            .get_lts_speed(EdgeId(99), base_speed, &model.mapping)
            .unwrap_err();
        assert!(matches!(
            oob_err,
            TraversalModelError::TraversalModelFailure(_)
        ));

        // Missing LTS entry in mapping fails during search
        let mut partial_map = std::collections::HashMap::new();
        partial_map.insert(
            Lts::new(1).unwrap(),
            LtsOperationConfig::PercentPenalty { percent: 100.0 },
        );
        let partial_model = LtsModel::new(
            engine.clone(),
            LtsParams {
                mapping: Some(LtsMappingConfig(partial_map)),
            },
        )
        .unwrap();
        let unmapped_err = engine
            .get_lts_speed(EdgeId(3), base_speed, &partial_model.mapping)
            .unwrap_err();
        assert!(matches!(
            unmapped_err,
            TraversalModelError::TraversalModelFailure(_)
        ));
    }

    #[test]
    fn test_compile_mapping_precedence() {
        let config_mapping = default_mapping_config();
        let mut override_map = std::collections::HashMap::new();
        override_map.insert(
            Lts::new(4).unwrap(),
            LtsOperationConfig::PercentPenalty { percent: 25.0 },
        );
        let params_mapping = LtsMappingConfig(override_map);

        let base_speed = Velocity::new::<uom::si::velocity::mile_per_hour>(10.0);

        // (Some(config), None) uses config
        let m1 = super::compile_mapping(Some(&config_mapping), None).unwrap();
        let s1 = m1[&Lts::new(4).unwrap()]
            .apply(base_speed)
            .get::<uom::si::velocity::mile_per_hour>();
        assert!((s1 - 5.0).abs() < 1e-9);

        // (None, Some(params)) uses params
        let m2 = super::compile_mapping(None, Some(&params_mapping)).unwrap();
        let s2 = m2[&Lts::new(4).unwrap()]
            .apply(base_speed)
            .get::<uom::si::velocity::mile_per_hour>();
        assert!((s2 - 2.5).abs() < 1e-9);

        // (Some(config), Some(params)) overrides with params
        let m3 = super::compile_mapping(Some(&config_mapping), Some(&params_mapping)).unwrap();
        assert_eq!(m3.len(), 1);
        let s3 = m3[&Lts::new(4).unwrap()]
            .apply(base_speed)
            .get::<uom::si::velocity::mile_per_hour>();
        assert!((s3 - 2.5).abs() < 1e-9);

        // (None, None) errors
        assert!(super::compile_mapping(None, None).is_err());
    }

    #[test]
    fn test_lts_model_traverse_edge_and_estimate_traversal() {
        let file = write_temp_lts_file(&["1", "4"]);
        let builder = LtsBuilder {};
        let config_json = json!({
            "lts_input_file": file.path().to_str().unwrap(),
            "speed_unit": "mph",
            "mapping": {
                "1": { "type": "percent_penalty", "percent": 100.0 },
                "2": { "type": "percent_penalty", "percent": 90.0 },
                "3": { "type": "percent_penalty", "percent": 75.0 },
                "4": { "type": "percent_penalty", "percent": 50.0 }
            }
        });
        let service = builder.build(&config_json).unwrap();
        let model = service.build(&json!({})).unwrap();

        let mut features = vec![(
            EDGE_SPEED.to_string(),
            StateVariableConfig::Speed {
                initial: Velocity::ZERO,
                accumulator: false,
                output_unit: Some(SpeedUnit::MPH),
            },
        )];
        features.extend(model.output_features());
        let state_model = StateModel::new(features);
        let mut state = state_model.initial_state(None).unwrap();

        let base_speed = Velocity::new::<uom::si::velocity::mile_per_hour>(12.0);
        state_model
            .set_speed(&mut state, EDGE_SPEED, &base_speed)
            .unwrap();

        let src = Vertex::new(0, 0.0, 0.0);
        let dst = Vertex::new(1, 0.01, 0.0);
        let edge = Edge::new(0, 1, 0, 1, Length::new::<uom::si::length::meter>(100.0));
        let label = Label::Vertex(VertexId(0));
        let tree = SearchTree::default();
        let ctx = EdgeFrontierContext::new(&label, &src, &edge, &dst, &tree);

        // Edge 1 has LTS 4 -> 50% penalty -> 6.0 mph in EDGE_LTS_SPEED
        model.traverse_edge(&ctx, &mut state, &state_model).unwrap();
        let lts_speed = state_model
            .get_speed(&state, LtsModel::EDGE_LTS_SPEED)
            .unwrap()
            .get::<uom::si::velocity::mile_per_hour>();
        assert!((lts_speed - 6.0).abs() < 1e-9);

        // estimate_traversal copies unpenalized EDGE_SPEED (12.0 mph)
        model
            .estimate_traversal((&src, &dst), &mut state, &tree, &state_model)
            .unwrap();
        let est_speed = state_model
            .get_speed(&state, LtsModel::EDGE_LTS_SPEED)
            .unwrap()
            .get::<uom::si::velocity::mile_per_hour>();
        assert!((est_speed - 12.0).abs() < 1e-9);
    }

    #[test]
    fn test_lts_invalid_file_values_fail() {
        let builder = LtsBuilder {};

        for bad_val in ["0", "5", "abc"] {
            let file = write_temp_lts_file(&["1", bad_val]);
            let res = builder.build(&json!({
                "lts_input_file": file.path().to_str().unwrap(),
                "speed_unit": "mph"
            }));
            assert!(res.is_err(), "expected failure for LTS value '{bad_val}'");
        }
    }
}
