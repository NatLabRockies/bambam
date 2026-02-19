use super::multimodal_traversal_ops as ops;
use crate::model::{
    bambam_state,
    state::{
        fieldname, multimodal_state_ops, multimodal_state_ops as state_ops, variable, LegIdx,
        MultimodalMapping, MultimodalStateMapping,
    },
};
use itertools::Itertools;
use routee_compass_core::{
    algorithm::search::SearchTree,
    model::{
        label::Label,
        network::{Edge, Vertex, VertexId},
        state::{InputFeature, StateModel, StateModelError, StateVariable, StateVariableConfig},
        traversal::{TraversalModel, TraversalModelError},
    },
};
use serde_json::json;
use std::sync::Arc;
use uom::si::f64::{Length, Time};

/// maps edge_time values to the correct mode and leg accumulators during traversal.
///
/// while the broader design of bambam assumes one travel mode per edge list, this model
/// instead assumes it can use some shared notion of a mapping from mode name to a numeric label
/// across edge lists.
pub struct MultimodalTraversalModel {
    pub mode: String,
    pub max_trip_legs: u64,
    pub mode_to_state: Arc<MultimodalStateMapping>,
    pub route_id_to_state: Arc<Option<MultimodalStateMapping>>,
}

/// Applies the multimodal leg + mode-specific accumulator updates during
/// edge_traversal.
impl TraversalModel for MultimodalTraversalModel {
    fn name(&self) -> String {
        format!("Multimodal Traversal Model ({})", self.mode)
    }

    fn input_features(&self) -> Vec<InputFeature> {
        let mut features = vec![
            InputFeature::Distance {
                name: fieldname::EDGE_DISTANCE.to_string(),
                unit: None,
            },
            InputFeature::Time {
                name: fieldname::EDGE_TIME.to_string(),
                unit: None,
            },
        ];
        if self.route_id_to_state.is_some() {
            features.push(variable::route_id_input_feature());
        }
        features
    }

    fn output_features(&self) -> Vec<(String, StateVariableConfig)> {
        let active_leg = std::iter::once((
            fieldname::ACTIVE_LEG.to_string(),
            variable::active_leg_variable_config(),
        ));
        let leg_mode = (0..self.max_trip_legs).map(|idx| {
            let name = fieldname::leg_mode_fieldname(idx);
            let config = variable::leg_mode_variable_config();
            (name, config)
        });

        let leg_dist = (0..self.max_trip_legs).map(|idx| {
            let name = fieldname::leg_distance_fieldname(idx);
            let config = variable::multimodal_distance_variable_config(None);
            (name, config)
        });
        let leg_time = (0..self.max_trip_legs).map(|idx| {
            let name = fieldname::leg_time_fieldname(idx);
            let config = variable::multimodal_time_variable_config(None);
            (name, config)
        });

        let leg_route_id: Box<dyn Iterator<Item = (String, StateVariableConfig)>> =
            if self.route_id_to_state.is_some() {
                Box::new((0..self.max_trip_legs).map(|idx| {
                    let name = fieldname::leg_route_id_fieldname(idx);
                    let config = variable::route_id_variable_config();
                    (name, config)
                }))
            } else {
                Box::new(std::iter::empty::<(String, StateVariableConfig)>())
            };

        let mode_dist = self.mode_to_state.get_categories().iter().map(|mode| {
            let name = fieldname::mode_distance_fieldname(mode);
            let config = variable::multimodal_distance_variable_config(None);
            (name, config)
        });

        let mode_time = self.mode_to_state.get_categories().iter().map(|mode| {
            let name = fieldname::mode_time_fieldname(mode);
            let config = variable::multimodal_time_variable_config(None);
            (name, config)
        });
        active_leg
            .chain(leg_mode)
            .chain(leg_dist)
            .chain(leg_time)
            .chain(leg_route_id)
            .chain(mode_dist)
            .chain(mode_time)
            .collect_vec()
    }

    fn traverse_edge(
        &self,
        trajectory: (&Vertex, &Edge, &Vertex),
        state: &mut Vec<StateVariable>,
        tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        let (_, edge, _) = trajectory;
        log::debug!(
            "begin multimodal traversal along edge {:?} for state at time: {:.2} minutes with tree size {}",
            (edge.edge_list_id, edge.edge_id),
            state_model
                .get_time(state, "trip_time")
                .unwrap_or_default()
                .get::<uom::si::time::minute>(),
            tree.len()
        );

        // first, apply any mode switching for using this edge
        ops::mode_switch(
            state,
            state_model,
            &self.mode,
            &self.mode_to_state,
            self.max_trip_legs,
        )?;

        // update multimodal mode + leg state
        let leg_idx = state_ops::get_active_leg_idx(state, state_model)?
            .ok_or_else(|| state_ops::error_inactive_state_traversal(state, state_model))?;
        ops::update_accumulators(
            state,
            state_model,
            &self.mode,
            leg_idx,
            &self.mode_to_state,
            self.max_trip_legs,
        )?;
        if let Some(route_id_to_state) = self.route_id_to_state.as_ref() {
            ops::update_route_id(
                state,
                state_model,
                &self.mode,
                leg_idx,
                route_id_to_state,
                self.max_trip_legs,
            )?;
        }
        log::debug!(
            "finish multimodal traversal along edge {:?} for state at time: {:.2} minutes with tree size {}",
            (edge.edge_list_id, edge.edge_id),
            state_model
                .get_time(state, "trip_time")
                .unwrap_or_default()
                .get::<uom::si::time::minute>(),
            tree.len()
        );
        Ok(())
    }

    fn estimate_traversal(
        &self,
        od: (&Vertex, &Vertex),
        state: &mut Vec<StateVariable>,
        tree: &SearchTree,
        state_model: &StateModel,
    ) -> Result<(), TraversalModelError> {
        // does not support A*-style estimation
        Ok(())
    }
    //
}

impl MultimodalTraversalModel {
    /// builds a new traversal model, associated with a specific edge list and travel mode.
    /// compatible with mode mappings shared from the upstream traversal model service or
    /// built just for this case.
    pub fn new(
        mode: String,
        max_trip_legs: u64,
        mode_to_state: Arc<MultimodalStateMapping>,
        route_id_to_state: Arc<Option<MultimodalStateMapping>>,
    ) -> MultimodalTraversalModel {
        Self {
            mode,
            max_trip_legs,
            mode_to_state,
            route_id_to_state,
        }
    }

    /// builds a new [`MultimodalTripLegModel`] from its data dependencies only.
    /// used in synchronous contexts like scripting or testing.
    pub fn new_local(
        mode: &str,
        max_trip_legs: u64,
        modes: &[&str],
        route_ids: &[&str],
    ) -> Result<MultimodalTraversalModel, StateModelError> {
        let mode_to_state =
            MultimodalMapping::new(&modes.iter().map(|s| s.to_string()).collect::<Vec<String>>())
                .map_err(|e| {
                StateModelError::BuildError(format!(
                    "while building MultimodalTripLegModel, failure constructing mode mapping: {e}"
                ))
            })?;

        let route_id_to_state = match route_ids {
            [] => None,
            _ => {
                let mapping = MultimodalMapping::new(
                    &route_ids
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<String>>(),
                )
                .map_err(|e| {
                    StateModelError::BuildError(format!(
                "while building MultimodalTripLegModel, failure constructing mode mapping: {e}"
            ))
                })?;
                Some(mapping)
            }
        };

        let mmm = MultimodalTraversalModel::new(
            mode.to_string(),
            max_trip_legs,
            Arc::new(mode_to_state),
            Arc::new(route_id_to_state),
        );
        Ok(mmm)
    }

    /// modifies a state serialization so that values related to multimodal access modeling
    /// have been re-mapped to their categorical values
    pub fn serialize_mapping_values(
        &self,
        state_json: &mut serde_json::Value,
        state: &[StateVariable],
        state_model: &StateModel,
        accumulators_only: bool,
    ) -> Result<(), StateModelError> {
        // use mappings to map any multimodal state values to their respective categoricals
        for idx in (0..self.max_trip_legs) {
            // re-map leg mode
            let mode_key = fieldname::leg_mode_fieldname(idx);
            ops::apply_mapping_for_serialization(state_json, &mode_key, idx, &self.mode_to_state)?;

            if let Some(route_id_to_state) = self.route_id_to_state.clone().as_ref() {
                let route_key = fieldname::leg_route_id_fieldname(idx);
                ops::apply_mapping_for_serialization(
                    state_json,
                    &route_key,
                    idx,
                    route_id_to_state,
                )?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::MultimodalTraversalModel;
    use crate::model::{
        label::multimodal::MultimodalLabelModel,
        state::{
            fieldname, multimodal_state_ops as state_ops, variable, LegIdx, MultimodalMapping,
            MultimodalStateMapping,
        },
    };
    use routee_compass_core::model::cost::{
        cost_model_service::CostModelService, CostModel, VehicleCostRate,
    };
    use routee_compass_core::{
        algorithm::search::{EdgeTraversal, SearchTree},
        model::{
            label::LabelModel,
            network::{Edge, Vertex},
            state::{StateModel, StateVariable},
            traversal::TraversalModel,
        },
        testing::mock::traversal_model::TestTraversalModel,
    };
    use std::{collections::HashMap, sync::Arc};
    use uom::si::f64::{Length, Time};

    // an initialized trip that has not begun should have active leg of None and
    // leg_0_mode of None.
    #[test]
    fn test_initialize_trip_access() {
        let test_mode = "walk";
        let max_trip_legs = 1;
        let mtm =
            MultimodalTraversalModel::new_local("walk", max_trip_legs, &["walk"], &["A", "B", "C"])
                .expect("test invariant failed, model constructor had error");
        let state_model = StateModel::new(mtm.output_features());
        let route_id_to_state = MultimodalStateMapping::empty(); // no route ids

        let mut state = state_model
            .initial_state(None)
            .expect("test invariant failed: unable to create state");

        // ASSERTION 1: there should be no active leg index, no trip has started.
        assert_active_leg(None, &state, &state_model).expect("assertion 1 failed");

        // ASSERTION 2: as we have no active leg index, the state vector should be in it's
        // initial state (empty or zero-valued state on leg 1).
        let leg_mode = state_ops::get_leg_mode_label(&state, 0, &state_model, max_trip_legs)
            .expect("test failed: did not find leg mode for leg 0");
        let leg_distance = state_ops::get_leg_distance(&state, 0, &state_model)
            .expect("test failed: did not find leg distance for leg 0");
        let leg_time = state_ops::get_leg_time(&state, 0, &state_model)
            .expect("test failed: did not find leg time for leg 0");
        let leg_route_id = state_ops::get_leg_route_id(&state, 0, &state_model, &route_id_to_state)
            .expect("test failed: did not find leg route id for leg 0");
        assert_eq!(leg_mode, None);
        assert_eq!(leg_distance.value, 0.0);
        assert_eq!(leg_time.value, 0.0);
        assert_eq!(leg_route_id, None);
        assert_eq!(leg_distance.value, 0.0);
    }

    // in a scenario with walk and bike mode, using an AccessModel for walk mode,
    // if we start a trip, we should assign 'walk' to the first leg and the active
    // leg should be 0.
    #[test]
    fn test_start_trip_access() {
        let test_mode = "walk";
        let max_trip_legs = 1;
        let (mtm, test_tm, state_model, mut state) =
            build_test_assets(&["walk"], &[], max_trip_legs, test_mode);

        let t1 = mock_trajectory(0, 0, 0);
        let mut tree = SearchTree::default();

        mtm.traverse_edge((&t1.0, &t1.1, &t1.2), &mut state, &tree, &state_model)
            .expect("access failed");

        // ASSERTION 1: by accessing a traversal, we must have transitioned from our initial state
        // to a state with exactly one trip leg.
        assert_active_leg(Some(0), &state, &state_model).expect("assertion 1 failed");

        // ASSERTION 2: the trip leg should be associated with the mode that the AccessModel sets.
        assert_active_mode(Some(test_mode), &state, &state_model, 1, &mtm.mode_to_state)
            .expect("assertion 2 failed");
    }

    #[test]
    fn test_switch_trip_mode_access() {
        // simulate two edge lists each with a mode-specific multimodal traversal model
        let max_trip_legs = 2;
        let (mtm_walk, test_walk, state_model, initial_state) =
            build_test_assets(&["bike", "walk"], &[], max_trip_legs, "walk");
        let (mtm_bike, test_bike, _, _) =
            build_test_assets(&["bike", "walk"], &[], max_trip_legs, "bike");
        let state_model = Arc::new(state_model);

        // assuming we can use mtm_walk and mtm_bike fields interchangeably
        assert_eq!(
            mtm_walk.output_features(),
            mtm_bike.output_features(),
            "test invariant failed: models should have matching state features"
        );

        let mut tree = SearchTree::default();
        let lm = MultimodalLabelModel::new(mtm_walk.mode_to_state.as_ref().clone(), max_trip_legs);

        // build state model and initial search state
        let cost_model = mock_cost_model(state_model.clone());

        // access edge 2 in walk mode, access edge 3 in bike mode
        // (0) -[0]-> (1) -[1]-> (2) -[2]-> (3) where
        //   - edge list 0 has edges 0 and 1, uses walk-mode access model
        //   - edge list 1 has edge 2, uses bike-mode access model
        let t1 = mock_trajectory(0, 0, 0);

        // traverse walk edge
        let et1 = EdgeTraversal::new_local(
            (&t1.0, &t1.1, &t1.2),
            &tree,
            &initial_state,
            &state_model,
            test_walk.as_ref(),
            &cost_model,
        )
        .expect("failed to traverse walk edge");

        // ASSERTION 1: trip enters "walk" mode after accessing edge 1 on edge list 0
        assert_active_leg(Some(0), &et1.result_state, &state_model).expect("assertion 1 failed");
        assert_active_mode(
            Some("walk"),
            &et1.result_state,
            &state_model,
            2,
            &mtm_walk.mode_to_state.clone(),
        )
        .expect("assertion 1 failed");

        // update tree with walk traversal
        let t1_src = lm
            .label_from_state(t1.0.vertex_id, &initial_state, &state_model)
            .expect("invariant failed: unable to create label for vertex 1");
        let t1_dst = lm
            .label_from_state(t1.2.vertex_id, &et1.result_state, &state_model)
            .expect("invariant failed: unable to create label for vertex 2");
        tree.insert(t1_src, et1.clone(), t1_dst);

        // traverse bike edge
        let t2 = mock_trajectory(1, 1, 1);
        let et2 = EdgeTraversal::new_local(
            (&t2.0, &t2.1, &t2.2),
            &tree,
            &et1.result_state,
            &state_model,
            test_bike.as_ref(),
            &cost_model,
        )
        .expect("failed to traverse bike edge");

        // ASSERTION 2: trip enters "bike" mode after accessing edge 2 on edge list 1
        assert_active_leg(Some(1), &et2.result_state, &state_model).expect("assertion 2 failed");
        assert_active_mode(
            Some("bike"),
            &et2.result_state,
            &state_model,
            2,
            &mtm_bike.mode_to_state,
        )
        .expect("assertion 2 failed");
    }

    #[test]
    fn test_switch_exceeds_max_legs() {
        // create an access model for two edge lists, "walk" and "bike" topology
        // but, here, we limit trip legs to 1, so our trip should not be able to transition to bike
        let max_trip_legs = 1;
        let (mtm_walk, test_walk, state_model, initial_state) =
            build_test_assets(&["bike", "walk"], &[], max_trip_legs, "walk");
        let (mtm_bike, test_bike, _, _) =
            build_test_assets(&["bike", "walk"], &[], max_trip_legs, "bike");
        let state_model = Arc::new(state_model);

        // build state model and initial search state
        assert_eq!(
            mtm_walk.output_features(),
            mtm_bike.output_features(),
            "test invariant failed: models should have matching state features"
        );
        let cost_model = mock_cost_model(state_model.clone());
        let mut tree = SearchTree::default();
        let lm = MultimodalLabelModel::new(mtm_walk.mode_to_state.as_ref().clone(), max_trip_legs);

        // the two trajectories concatenate together into the sequence
        // (0) -[0]-> (1) -[1]-> (2) -[2]-> (3)
        // where
        //   - edge list 0 has edges 0 and 1, uses walk-mode access model
        //   - edge list 1 has edge 2, uses bike-mode access model
        let t1 = mock_trajectory(0, 0, 0);
        let t2 = mock_trajectory(1, 1, 1);

        // establish the trip state on "walk"-mode travel
        let et1 = EdgeTraversal::new_local(
            (&t1.0, &t1.1, &t1.2),
            &tree,
            &initial_state,
            &state_model,
            test_walk.as_ref(),
            &cost_model,
        )
        .expect("failed to traverse walk edge");

        // update tree with walk traversal
        let t1_src = lm
            .label_from_state(t1.0.vertex_id, &initial_state, &state_model)
            .expect("invariant failed: unable to create label for vertex 1");
        let t1_dst = lm
            .label_from_state(t1.2.vertex_id, &et1.result_state, &state_model)
            .expect("invariant failed: unable to create label for vertex 2");
        tree.insert(t1_src, et1.clone(), t1_dst);

        // ASSERTION 1: trip tries to enter "bike" mode after accessing edge 2 on edge list 1,
        // but this should result in an error, as we have restricted the max number of trip legs to 1.
        let result = EdgeTraversal::new_local(
            (&t2.0, &t2.1, &t2.2),
            &tree,
            &et1.result_state,
            &state_model,
            test_bike.as_ref(),
            &cost_model,
        );
        match result {
            Ok(e) => panic!("assertion 2 failed, should have been an error"),
            Err(e) => assert!(format!("{e}").contains("invalid leg id 1 >= max leg id 1")),
        }
    }

    #[test]
    fn test_initialize_trip_traversal() {
        let available_modes = ["walk", "bike", "drive"];
        let max_trip_legs = 4;
        let this_mode = "walk";

        let (tm, test_tm, state_model, state) =
            build_test_assets(&available_modes, &["A", "B", "C"], max_trip_legs, this_mode);
        let mapping = MultimodalStateMapping::empty(); // no route ids

        // as a head check, we can also inspect the serialized access state JSON in the logs
        print_state(&state, &state_model);

        // ASSERTION 1: state has the expected length given the provided number of trip legs + modes
        let expected_len = {
            let active_leg = 1;
            let route_id = 1;
            let input_features = 2; // edge_time, trip_time
            let leg_fields = 4; // mode, distance, time, route_id
            let mode_fields = 2;
            active_leg
                + route_id
                + input_features
                + available_modes.len() * mode_fields
                + max_trip_legs as usize * leg_fields
        };
        assert_eq!(state.len(), expected_len);

        // ASSERTION 2: confirm each leg's dist/time keys exist and values were set with zeroes
        for leg_idx in (0..max_trip_legs) {
            let dist = state_ops::get_leg_distance(&state, leg_idx, &state_model)
                .unwrap_or_else(|_| panic!("unable to get leg distance for leg {leg_idx}"));
            let time = state_ops::get_leg_time(&state, leg_idx, &state_model)
                .unwrap_or_else(|_| panic!("unable to get leg time for leg {leg_idx}"));
            let route_id = state_ops::get_leg_route_id(&state, leg_idx, &state_model, &mapping)
                .unwrap_or_else(|_| panic!("unable to get leg route_id for leg {leg_idx}"));
            assert_eq!(dist.value, 0.0);
            assert_eq!(time.value, 0.0);
            assert_eq!(route_id, None);
        }
    }

    #[test]
    fn test_start_trip_traversal() {
        let available_modes = ["walk"];
        let max_trip_legs = 1;
        let this_mode = "walk";
        let (tm, test_tm, state_model, mut state) =
            build_test_assets(&available_modes, &[], max_trip_legs, this_mode);
        let tree = SearchTree::default();

        // mock up some edge_dist, edge_time values
        let distance = Length::new::<uom::si::length::mile>(3.14159);
        state_model
            .set_distance(&mut state, "edge_distance", &distance)
            .expect("test invariant failed: could not assign edge_distance");
        let time = Time::new::<uom::si::time::minute>(60.0);
        state_model
            .set_time(&mut state, "edge_time", &time)
            .expect("test invariant failed: could not assign edge_time");

        // let's traverse! topology: (0) -[0]-> (1), 1km edge
        let t = mock_trajectory(0, 0, 0);

        test_tm
            .traverse_edge((&t.0, &t.1, &t.2), &mut state, &tree, &state_model)
            .expect("failed to traverse edge");

        // as a head check, we can also inspect the serialized access state JSON in the logs
        print_state(&state, &state_model);

        // ASSERTION 1: values copied to leg + mode accumulators should be correct
        let leg_0_distance =
            state_ops::get_leg_distance(&state, 0, &state_model).expect("should find leg distance");
        let leg_0_time =
            state_ops::get_leg_time(&state, 0, &state_model).expect("should find leg time");
        let mode_walk_distance = state_ops::get_mode_distance(&state, "walk", &state_model)
            .expect("should find mode distance");
        let mode_walk_time =
            state_ops::get_mode_time(&state, "walk", &state_model).expect("should find mode time");
        assert_eq!(leg_0_distance, distance);
        assert_eq!(leg_0_time, time);
        assert_eq!(mode_walk_distance, distance);
        assert_eq!(mode_walk_time, time);
    }

    /// creates all of the required test assets, where
    ///   - tm is the MultimodalTraversalModel value
    ///   - test_tm is the model concatenated with the TestTraversalModel to enable
    ///     use of the edge_traversal method
    ///   - state_model is the state model built from the test_tm
    ///   - state is the initial state built from the state_model
    fn build_test_assets(
        available_modes: &[&str],
        available_route_ids: &[&str],
        max_trip_legs: u64,
        this_mode: &str,
    ) -> (
        Arc<MultimodalTraversalModel>,
        Arc<dyn TraversalModel>,
        StateModel,
        Vec<StateVariable>,
    ) {
        let tm = Arc::new(
            MultimodalTraversalModel::new_local(
                this_mode,
                max_trip_legs,
                available_modes,
                available_route_ids,
            )
            .expect("test invariant failed, model constructor had error"),
        );
        let test_tm = TestTraversalModel::new(tm.clone())
            .expect("test invariant failed, unable to produce a test model");

        let state_model = StateModel::new(test_tm.output_features());

        let mut state = state_model
            .initial_state(None)
            .expect("test invariant failed: state model could not create initial state");
        (tm, test_tm, state_model, state)
    }

    /// helper to create trajectories spaced apart evenly along a line with segments of uniform length
    fn mock_trajectory(
        start_vertex: usize,
        edge_id: usize,
        edge_list_id: usize,
    ) -> (Vertex, Edge, Vertex) {
        let v1 = start_vertex;
        let v2 = v1 + 1;
        let x1 = (v1 as f32) * 0.01;
        let x2 = (v2 as f32) * 0.01;

        (
            Vertex::new(v1, x1, 0.0),
            Edge::new(
                edge_list_id,
                edge_id,
                v1,
                v2,
                Length::new::<uom::si::length::meter>(1000.0),
            ),
            Vertex::new(v2, x2, 0.0),
        )
    }

    fn mock_cost_model(state_model: Arc<StateModel>) -> Arc<CostModel> {
        let weights_mapping = state_model
            .iter()
            .map(|(n, _)| (n.to_string(), 1.0))
            .collect::<HashMap<_, _>>();
        let vehicle_rate_mapping = state_model
            .iter()
            .map(|(n, _)| (n.to_string(), VehicleCostRate::Raw))
            .collect::<HashMap<_, _>>();
        let result = CostModel::new(
            Arc::new(weights_mapping),
            Arc::new(vehicle_rate_mapping),
            Arc::new(HashMap::new()),
            routee_compass_core::model::cost::CostAggregation::Sum,
            state_model,
        )
        .expect("test invariant failed: unable to build cost model");
        Arc::new(result)
    }

    fn assert_active_leg(
        leg_idx: Option<LegIdx>,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<(), String> {
        let active_leg = state_ops::get_active_leg_idx(state, state_model)
            .expect("failure getting active leg index");

        match (leg_idx, active_leg) {
            (None, None) => {
                // no active leg testing against no active mode, ok
                Ok(())
            }
            (None, Some(leg_idx)) => {
                Err(format!("assert_active_leg failure: we are expecting no active leg, but state has leg index of {leg_idx}"))
            }
            (Some(idx), None) => {
                Err(format!("assert_active_leg failure: we are expecting active leg index {idx}, but state has no active leg"))
            }
            (Some(test_idx), Some(active_leg_idx)) => {
                if test_idx != active_leg_idx {
                    Err(format!("expected active leg index of {active_leg_idx} to be {test_idx}"))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn assert_active_mode(
        mode: Option<&str>,
        state: &[StateVariable],
        state_model: &StateModel,
        max_trip_legs: u64,
        mode_to_state: &MultimodalStateMapping,
    ) -> Result<(), String> {
        let active_leg_opt = state_ops::get_active_leg_idx(state, state_model)
            .expect("failure getting active leg index");

        match (mode, active_leg_opt) {
            (None, None) => {
                // no active leg testing against no active mode, ok
                Ok(())
            }
            (None, Some(leg_idx)) => {
                Err(format!("assert_active_mode failure: we are expecting no active mode, but state has leg index of {leg_idx}"))
            }
            (Some(m), None) => {
                Err("assert_active_mode failure: we are expecting an active mode, but state has no active leg".to_string())
            }
            (Some(test_mode), Some(leg_idx)) => {
                let active_mode = state_ops::get_existing_leg_mode(state, leg_idx, state_model, max_trip_legs, mode_to_state)
                    .unwrap_or_else(|_| panic!("failure getting mode for leg {leg_idx}"));

                if active_mode != test_mode {
                    Err(format!("expected active leg mode of {active_mode} to be {test_mode}"))
                } else {
                    Ok(())
                }

            }
        }
    }

    /// helper for printing the state as JSON to the console
    fn print_state(state: &[StateVariable], state_model: &StateModel) {
        let state_json = state_model
            .serialize_state(state, false)
            .expect("state serialization failed");
        println!(
            "{}",
            serde_json::to_string_pretty(&state_json).unwrap_or_default()
        );
    }
}
