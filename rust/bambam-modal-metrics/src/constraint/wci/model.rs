use std::sync::Arc;

use super::{WciEngine, WciParams};

use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
    traversal::EdgeFrontierContext,
};

pub struct WciModel {
    pub engine: Arc<WciEngine>,
    pub params: WciParams,
}

impl WciModel {
    pub fn new(engine: Arc<WciEngine>, params: WciParams) -> Self {
        // modify this and the struct definition if additional pre-processing
        // is required during model instantiation from query parameters.
        Self { engine, params }
    }
}

impl ConstraintModel for WciModel {
    fn valid_frontier(
        &self,
        ctx: &EdgeFrontierContext,
        _state: &[StateVariable],
        _state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        self.engine
            .accept_edge_wci(ctx.edge, self.params.wci_threshold)
    }

    fn valid_edge(&self, edge: &Edge) -> Result<bool, ConstraintModelError> {
        self.engine.accept_edge_wci(edge, self.params.wci_threshold)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;

    use routee_compass_core::{
        algorithm::search::SearchTree,
        model::{
            constraint::ConstraintModelBuilder,
            label::Label,
            network::{Edge, Vertex, VertexId},
            state::StateModel,
            traversal::EdgeFrontierContext,
        },
    };
    use serde_json::json;
    use tempfile::NamedTempFile;
    use uom::si::f64::Length;

    use crate::{
        constraint::wci::WciBuilder,
        wci::wci::{Wci, MAX_WCI, MIN_WCI},
    };

    use super::*;

    fn write_temp_wci_file(lines: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file
    }

    fn mock_edge(edge_id: usize) -> Edge {
        Edge::new(0, edge_id, 0, 1, Length::new::<uom::si::length::meter>(100.0))
    }

    #[test]
    fn test_accept_edge_wci_boundary_conditions() {
        let file = write_temp_wci_file(&["-6", "-1", "0", "1", "9"]);
        let config = super::super::WciConfig {
            wci_input_file: file.path().to_str().unwrap().to_string(),
        };
        let engine = WciEngine::try_from(config).unwrap();
        let threshold = Wci::new(0).unwrap();

        assert!(!engine.accept_edge_wci(&mock_edge(0), threshold).unwrap());
        assert!(!engine.accept_edge_wci(&mock_edge(1), threshold).unwrap());
        assert!(engine.accept_edge_wci(&mock_edge(2), threshold).unwrap());
        assert!(engine.accept_edge_wci(&mock_edge(3), threshold).unwrap());
        assert!(engine.accept_edge_wci(&mock_edge(4), threshold).unwrap());
    }

    #[test]
    fn test_accept_edge_wci_missing_edge_id_errors() {
        let file = write_temp_wci_file(&["-1", "0", "2"]);
        let config = super::super::WciConfig {
            wci_input_file: file.path().to_str().unwrap().to_string(),
        };
        let engine = WciEngine::try_from(config).unwrap();

        let err = engine
            .accept_edge_wci(&mock_edge(99), Wci::new(0).unwrap())
            .unwrap_err();
        assert!(matches!(err, ConstraintModelError::ConstraintModelError(_)));
    }

    #[test]
    fn test_wci_builder_service_model_lifecycle() {
        let file = write_temp_wci_file(&[&MIN_WCI.to_string(), "-1", "0", &MAX_WCI.to_string()]);
        let builder = WciBuilder {};
        let config_json = json!({
            "wci_input_file": file.path().to_str().unwrap()
        });
        let service = builder.build(&config_json).unwrap();

        let state_model = Arc::new(StateModel::new(vec![]));
        let query_json = json!({
            "wci_threshold": 0
        });
        let model = service.build(&query_json, state_model.clone()).unwrap();

        let state = state_model.initial_state(None).unwrap();
        let src = Vertex::new(0, 0.0, 0.0);
        let dst = Vertex::new(1, 0.01, 0.0);
        let label = Label::Vertex(VertexId(0));
        let tree = SearchTree::default();

        let edge_neg = mock_edge(1);
        let ctx_neg = EdgeFrontierContext::new(&label, &src, &edge_neg, &dst, &tree);
        assert!(!model.valid_edge(&edge_neg).unwrap());
        assert!(!model.valid_frontier(&ctx_neg, &state, &state_model).unwrap());

        let edge_zero = mock_edge(2);
        let ctx_zero = EdgeFrontierContext::new(&label, &src, &edge_zero, &dst, &tree);
        assert!(model.valid_edge(&edge_zero).unwrap());
        assert!(model
            .valid_frontier(&ctx_zero, &state, &state_model)
            .unwrap());
    }

    #[test]
    fn test_wci_invalid_file_and_query_params_fail() {
        let builder = WciBuilder {};

        // Out-of-range WCI value in file (-10 < MIN_WCI)
        let out_of_range_file = write_temp_wci_file(&["0", "-10"]);
        let res = builder.build(&json!({
            "wci_input_file": out_of_range_file.path().to_str().unwrap()
        }));
        assert!(res.is_err());

        // Non-integer WCI value in file
        let non_int_file = write_temp_wci_file(&["0", "not_a_number"]);
        let res = builder.build(&json!({
            "wci_input_file": non_int_file.path().to_str().unwrap()
        }));
        assert!(res.is_err());

        // Valid file, invalid query threshold (99 > MAX_WCI) or missing threshold
        let valid_file = write_temp_wci_file(&["0", "1"]);
        let service = builder
            .build(&json!({
                "wci_input_file": valid_file.path().to_str().unwrap()
            }))
            .unwrap();
        let state_model = Arc::new(StateModel::new(vec![]));
        assert!(service
            .build(&json!({ "wci_threshold": 99 }), state_model.clone())
            .is_err());
        assert!(service.build(&json!({}), state_model).is_err());
    }
}
