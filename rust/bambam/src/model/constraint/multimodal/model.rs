use std::sync::Arc;

use crate::model::constraint::multimodal::{
    MultimodalConstraintConstraintConfig, MultimodalConstraintEngine,
};
use crate::model::state::{MultimodalMapping, MultimodalStateMapping};
use crate::model::{
    constraint::multimodal::MultimodalConstraintConstraint, state::multimodal_state_ops as state_ops,
};
use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
};

pub struct MultimodalConstraintModel {
    pub engine: Arc<MultimodalConstraintEngine>,
}

impl MultimodalConstraintModel {
    pub fn new(engine: Arc<MultimodalConstraintEngine>) -> Self {
        Self { engine }
    }

    /// builds a new [`MultimodalConstraintModel`] from its data dependencies only.
    /// used in synchronous contexts like scripting or testing.
    pub fn new_local(
        mode: &str,
        constraints: Vec<MultimodalConstraintConstraint>,
        modes: &[&str],
        route_ids: &[&str],
        max_trip_legs: u64,
        use_route_ids: bool,
    ) -> Result<Self, ConstraintModelError> {
        let mode_to_state =
            MultimodalMapping::new(&modes.iter().map(|s| s.to_string()).collect::<Vec<String>>())
                .map_err(|e| {
                ConstraintModelError::BuildError(format!(
                    "while building local MultimodalConstraintModel, failure constructing mode mapping: {e}"
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
                    ConstraintModelError::BuildError(format!(
                "while building MultimodalConstraintModel, failure constructing mode mapping: {e}"
            ))
                })?;
                Some(mapping)
            }
        };
        let engine = MultimodalConstraintEngine {
            mode: mode.to_string(),
            constraints,
            mode_to_state: Arc::new(mode_to_state),
            route_id_to_state: Arc::new(route_id_to_state),
            max_trip_legs,
        };

        let mmm = MultimodalConstraintModel::new(Arc::new(engine));
        Ok(mmm)
    }
}

impl ConstraintModel for MultimodalConstraintModel {
    /// confirms that, upon reaching this edge,
    ///   - we have not exceeded any mode-specific distance, time or energy limit
    /// confirms that, if we add this edge,
    ///   - we have not exceeded max trip legs
    ///   - we have not exceeded max mode counts
    ///   - our trip still matches any exact mode sequences
    fn valid_frontier(
        &self,
        edge: &Edge,
        previous_edge: Option<&Edge>,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        for constraint in self.engine.constraints.iter() {
            let valid = constraint.valid_frontier(
                &self.engine.mode,
                edge,
                state,
                state_model,
                &self.engine.mode_to_state,
                self.engine.max_trip_legs,
            )?;
            log::debug!(
                "multimodal frontier is valid? '{valid}' for state at time: {:.2} minutes",
                state_model
                    .get_time(state, "trip_time")
                    .unwrap_or_default()
                    .get::<uom::si::time::minute>()
            );
            if !valid {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn valid_edge(&self, edge: &Edge) -> Result<bool, ConstraintModelError> {
        Ok(true)
    }
}

#[cfg(test)]
mod test {
    use std::collections::{HashMap, HashSet};

    use itertools::Itertools;
    use routee_compass_core::model::{
        constraint::ConstraintModel,
        network::Edge,
        state::{StateModel, StateVariable},
        traversal::TraversalModel,
    };
    use uom::si::f64::Length;

    use crate::model::{
        constraint::multimodal::{
            model::MultimodalConstraintModel, sequence_trie::SubSequenceTrie,
            MultimodalConstraintConstraint,
        },
        state::{multimodal_state_ops as state_ops, MultimodalStateMapping},
        traversal::multimodal::MultimodalTraversalModel,
    };

    #[test]
    fn test_valid_max_trip_legs_empty_state() {
        // testing validitity of an initial state using constraint "max trip legs = 1"
        let max_trip_legs = 1;
        let (mam, mfm, state_model, state) = test_setup(
            vec![MultimodalConstraintConstraint::MaxTripLegs(1)],
            "walk",
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));

        // test
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_valid_n_legs() {
        // testing validitity of a state with one leg using constraint "max trip legs = 2"
        let max_trip_legs = 2;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![MultimodalConstraintConstraint::MaxTripLegs(1)],
            "walk",
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));

        // assign one leg to walk mode
        state_ops::set_leg_mode(&mut state, 0, "walk", &state_model, &mam.mode_to_state)
            .expect("test invariant failed");
        state_ops::increment_active_leg_idx(&mut state, &state_model, max_trip_legs)
            .expect("test invariant failed");

        // test
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_n_legs() {
        // testing validitity of a state with two legs using constraint "max trip legs = 1"
        let max_trip_legs = 2;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![MultimodalConstraintConstraint::MaxTripLegs(1)],
            "walk",
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        // assign one leg to walk mode
        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        inject_trip_legs(
            &["walk", "bike"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // test
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_valid_mode_counts() {
        // testing validitity of traversing a "walk" edge using state with "walk", "drive", "walk" sequence.
        // our constraint is walk<=2, drive<=1. since this new edge has walk-mode, it will not increase the
        // number of trip legs, so it should be valid.
        let max_trip_legs = 5;
        let mode_constraint = MultimodalConstraintConstraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 2),
            ("drive".to_string(), 1),
        ]));
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "drive", "walk"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // test adding another walk edge to this trip leg, which does not increase the mode counts for walk.
        let walk_edge_list = 0;
        let edge = Edge::new(
            walk_edge_list,
            0,
            0,
            1,
            Length::new::<uom::si::length::meter>(1000.0),
        );
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_mode_counts() {
        // testing validitity of traversing a "drive" edge using state with "walk", "drive", "walk" sequence.
        // our constraint is walk<=2, drive<=1. since this new edge has drive-mode, it will increase the
        // number of trip legs, so it should be invalid.
        let max_trip_legs = 5;
        let mode_constraint = MultimodalConstraintConstraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 2),
            ("drive".to_string(), 1),
        ]));
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "bike", "walk", "drive"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // test accessing another walk-mode link, which would increase the number of walk-mode legs to 3
        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_valid_allowed_modes() {
        // testing validitity of traversing a "walk" edge when the constraint allows only
        // "walk" and "transit" modes. this should be valid.
        let mode_constraint = MultimodalConstraintConstraint::AllowedModes(HashSet::from([
            "walk".to_string(),
            "transit".to_string(),
        ]));
        let max_trip_legs = 3;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit", "walk"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // test appending one more walk-mode edge, which will not modify the existing trip legs
        let walk_edge_list = 0;
        let edge = Edge::new(
            walk_edge_list,
            0,
            0,
            1,
            Length::new::<uom::si::length::meter>(1000.0),
        );
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_allowed_modes() {
        // testing validitity of traversing a "drive" edge when the constraint allows only
        // "walk" and "transit" modes. this should be invalid.
        let mode_constraint = MultimodalConstraintConstraint::AllowedModes(HashSet::from([
            "walk".to_string(),
            "transit".to_string(),
        ]));
        let max_trip_legs = 4;
        let (mtm, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "drive",
            &["walk", "bike", "drive", "tnc", "transit"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit", "walk"],
            &mut state,
            &state_model,
            &mtm.mode_to_state,
            max_trip_legs,
        );

        // test the drive-mode traversal model, which is not an allowed mode
        let edge = Edge::new(2, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_valid_subsequence_empty_state() {
        // testing validitity of traversing a "walk" edge for an initial state where "walk"
        // is a matching subsequence. should be valid.
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec![
            "walk".to_string(),
            "transit".to_string(),
            "walk".to_string(),
        ]);
        let mode_constraint = MultimodalConstraintConstraint::ExactSequences(trie);
        let max_trip_legs = 3;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            &[],
            max_trip_legs,
        );

        // test adding a walk edge to a state with no trip legs
        let walk_edge_list = 0;
        let edge = Edge::new(
            walk_edge_list,
            0,
            0,
            1,
            Length::new::<uom::si::length::meter>(1000.0),
        );
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_valid_subsequence() {
        // testing validitity of traversing a "walk" edge for a "walk"->"transit" state where "walk"
        // is a matching subsequence. should be valid.
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec![
            "walk".to_string(),
            "transit".to_string(),
            "walk".to_string(),
        ]);
        let mode_constraint = MultimodalConstraintConstraint::ExactSequences(trie);
        let max_trip_legs = 3;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // test traversing a walk-mode edge list. "walk" -> "transit" -> "walk" is a valid sequence.
        let walk_edge_list = 0;
        let edge = Edge::new(
            walk_edge_list,
            0,
            0,
            1,
            Length::new::<uom::si::length::meter>(1000.0),
        );
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_subsequence() {
        // testing validitity of traversing a "walk" edge for a "walk"->"transit" state where "walk"->"transit"->"walk"
        // is NOT a matching subsequence. should be invalid.
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec!["walk".to_string(), "transit".to_string()]);
        let mode_constraint = MultimodalConstraintConstraint::ExactSequences(trie);
        let max_trip_legs = 3;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            &[],
            max_trip_legs,
        );

        // edge list one is a walk-mode edge list
        let edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));

        inject_trip_legs(
            &["walk", "transit"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // test
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid);
    }

    /// helper function to set up MultimodalConstraintModel test case assets
    fn test_setup(
        constraints: Vec<MultimodalConstraintConstraint>,
        this_mode: &str,
        modes: &[&str],
        route_ids: &[&str],
        max_trip_legs: u64,
    ) -> (
        MultimodalTraversalModel,
        MultimodalConstraintModel,
        StateModel,
        Vec<StateVariable>,
    ) {
        let mtm = MultimodalTraversalModel::new_local(this_mode, max_trip_legs, modes, &[])
            .expect("test invariant failed");
        let state_model = StateModel::new(mtm.output_features());
        let mfm = MultimodalConstraintModel::new_local(
            this_mode,
            constraints,
            modes,
            route_ids,
            max_trip_legs,
            true,
        )
        .expect("test invariant failed");
        let state = state_model
            .initial_state(None)
            .expect("test invariant failed");

        (mtm, mfm, state_model, state)
    }

    fn inject_trip_legs(
        legs: &[&str],
        state: &mut [StateVariable],
        state_model: &StateModel,
        mode_to_state: &MultimodalStateMapping,
        max_trip_legs: u64,
    ) {
        for (leg_idx, mode) in legs.iter().enumerate() {
            state_ops::set_leg_mode(state, leg_idx as u64, mode, state_model, mode_to_state)
                .expect("test invariant failed");
            state_ops::increment_active_leg_idx(state, state_model, max_trip_legs)
                .expect("test invariant failed");
        }
    }

    #[test]
    fn test_max_trip_legs_zero() {
        // Test with max_trip_legs = 0 - empty state should be valid since it has 0 legs
        let max_trip_legs = 1;
        let (mam, mfm, state_model, state) = test_setup(
            vec![MultimodalConstraintConstraint::MaxTripLegs(0)],
            "walk",
            &["walk"],
            &[],
            max_trip_legs,
        );

        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid); // Should be valid for empty state since it has 0 legs and max is 0
    }

    #[test]
    fn test_mode_counts_zero_limit() {
        // Test mode count constraint with 0 limit for a mode
        let mode_constraint = MultimodalConstraintConstraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 0),
            ("bike".to_string(), 1),
        ]));
        let max_trip_legs = 2;
        let (walk_mtm, walk_mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "walk", // Start with bike mode to avoid walk
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        // Test that walk-mode edge is invalid when walk has 0 limit
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = walk_mfm
            .valid_frontier(&walk_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_mode_counts_mode_not_in_limits() {
        // Test edge for a mode that's not mentioned in the limits (should be invalid)
        let mode_constraint = MultimodalConstraintConstraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 2),
            ("bike".to_string(), 1),
        ]));
        let max_trip_legs = 3;
        let (mam, mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "drive",
            &["walk", "bike", "drive"], // drive is not in the limits
            &[],
            max_trip_legs,
        );

        // Test drive-mode edge traversal model when drive is not in limits
        let dummy_edge = Edge::new(2, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&dummy_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_mode_counts_same_mode_continuation() {
        // Test that continuing with the same mode doesn't increment the count
        let mode_constraint =
            MultimodalConstraintConstraint::ModeCounts(HashMap::from([("walk".to_string(), 1)]));
        let max_trip_legs = 2;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // Test adding another walk edge (same mode) - should be valid
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&walk_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_allowed_modes_empty_set() {
        // Test with empty allowed modes set (should reject all modes)
        let mode_constraint = MultimodalConstraintConstraint::AllowedModes(HashSet::new());
        let max_trip_legs = 2;
        let modes = [
            "walk", "bike", "drive", "tnc", "transit", "eBike", "eVTOL", "airplane", "ferry",
        ];
        let (mam, mfm, state_model, state) =
            test_setup(vec![mode_constraint], "walk", &modes, &[], max_trip_legs);

        for edge_list_id in (0..modes.len()) {
            let edge = Edge::new(
                edge_list_id,
                0,
                0,
                1,
                Length::new::<uom::si::length::meter>(1000.0),
            );
            let is_valid = mfm
                .valid_frontier(&edge, None, &state, &state_model)
                .expect("test failed");
            assert!(!is_valid);
        }
    }

    #[test]
    fn test_allowed_modes_case_sensitivity() {
        // Test that mode matching is case-sensitive
        let mode_constraint = MultimodalConstraintConstraint::AllowedModes(HashSet::from([
            "Walk".to_string(), // Note capital W
        ]));
        let max_trip_legs = 2;
        let (mam, mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "walk",            // lowercase
            &["walk", "Walk"], // Include both cases in modes
            &[],
            max_trip_legs,
        );

        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0)); // lowercase walk
        let is_valid = mfm
            .valid_frontier(&walk_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid); // Should be invalid due to case mismatch
    }

    #[test]
    fn test_exact_sequences_multiple_valid_sequences() {
        // Test with multiple valid sequences where one matches
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec!["walk".to_string(), "transit".to_string()]);
        trie.insert_sequence(vec!["bike".to_string(), "walk".to_string()]);
        let mode_constraint = MultimodalConstraintConstraint::ExactSequences(trie);
        let max_trip_legs = 3;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "transit"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["bike"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // Test walk edge - should be valid as "bike" -> "walk" is a valid sequence
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&walk_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_exact_sequences_empty_trie() {
        // Test with empty trie (should reject all sequences)
        let trie = SubSequenceTrie::new();
        let mode_constraint = MultimodalConstraintConstraint::ExactSequences(trie);
        let max_trip_legs = 2;
        let (mam, mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&walk_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_exact_sequences_partial_match_longer_sequence() {
        // Test partial match where we're in the middle of a longer valid sequence
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec![
            "walk".to_string(),
            "transit".to_string(),
            "bike".to_string(),
            "walk".to_string(),
        ]);
        let mode_constraint = MultimodalConstraintConstraint::ExactSequences(trie);
        let max_trip_legs = 5;
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "transit"],
            &[],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // Test bike edge - should be valid as we're continuing the valid sequence
        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&bike_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_multiple_constraints_all_valid() {
        // Test with multiple constraints where all should pass
        let max_trip_legs = 3;
        let constraints = vec![
            MultimodalConstraintConstraint::MaxTripLegs(2),
            MultimodalConstraintConstraint::AllowedModes(HashSet::from([
                "walk".to_string(),
                "bike".to_string(),
            ])),
            MultimodalConstraintConstraint::ModeCounts(HashMap::from([
                ("walk".to_string(), 2),
                ("bike".to_string(), 1),
            ])),
        ];
        let (mam, mfm, state_model, mut state) =
            test_setup(constraints, "walk", &["walk", "bike"], &[], max_trip_legs);

        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&bike_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_multiple_constraints_one_fails() {
        // Test with multiple constraints where one should fail
        let max_trip_legs = 3;
        let constraints = vec![
            MultimodalConstraintConstraint::MaxTripLegs(2),
            MultimodalConstraintConstraint::AllowedModes(HashSet::from([
                "walk".to_string(), // bike not allowed
            ])),
        ];
        let (bike_mtm, bike_mfm, state_model, mut state) =
            test_setup(constraints, "bike", &["walk", "bike"], &[], max_trip_legs);

        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &bike_mtm.mode_to_state,
            max_trip_legs,
        );

        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = bike_mfm
            .valid_frontier(&bike_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid); // Should fail due to AllowedModes constraint
    }

    #[test]
    fn test_large_mode_sequence() {
        // Test with a large number of trip legs to ensure performance
        let max_trip_legs = 100;
        let mode_constraint = MultimodalConstraintConstraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 25), // Lower limit to trigger the constraint
            ("bike".to_string(), 25),
        ]));
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        // Inject many trip legs - this will create 26 walk and 24 bike legs
        let large_sequence: Vec<&str> = (0..50)
            .map(|i| if i % 2 == 0 { "walk" } else { "bike" })
            .collect();
        inject_trip_legs(
            &large_sequence,
            &mut state,
            &state_model,
            &mam.mode_to_state,
            max_trip_legs,
        );

        // Since we have 26 walk legs and the limit is 25, another walk edge should be invalid
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = mfm
            .valid_frontier(&walk_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid); // Should be invalid as we've exceeded the walk limit
    }

    #[test]
    fn test_max_trip_legs_would_exceed_limit() {
        // Test transition from valid state to invalid state when adding a new mode
        let max_trip_legs = 1;
        let (bike_mtm, bike_mfm, state_model, mut state) = test_setup(
            vec![MultimodalConstraintConstraint::MaxTripLegs(1)],
            "bike",
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        // Set up state with exactly 1 leg (at the limit)
        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &bike_mtm.mode_to_state,
            max_trip_legs,
        );

        // Test adding a different mode edge, which would create a second leg and exceed the limit
        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = bike_mfm
            .valid_frontier(&bike_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid); // Should be invalid as this would create a second leg
    }

    #[test]
    fn test_max_trip_legs_same_mode_continuation_at_limit() {
        // Test that continuing with the same mode when at the limit is still invalid
        // This tests the bug fix where same-mode continuation was always returning 0 legs

        // max_trip_legs is the state buffer size, constraint is the actual limit
        let max_trip_legs = 2; // State buffer can hold 2 legs
        let constraint_limit = 1; // But we only allow 1 leg

        let (bike_mtm, bike_mfm, state_model, mut state) = test_setup(
            vec![MultimodalConstraintConstraint::MaxTripLegs(constraint_limit)],
            "bike", // ConstraintModel for bike edges
            &["walk", "bike"],
            &[],
            max_trip_legs,
        );

        // Set up state with 2 legs: walk then bike (exceeds constraint_limit of 1)
        inject_trip_legs(
            &["walk", "bike"],
            &mut state,
            &state_model,
            &bike_mtm.mode_to_state,
            max_trip_legs,
        );

        // Test continuing with bike-mode edge (same as active mode)
        // edge.edge_list_id doesn't matter since we're just checking constraints, not traversal
        // The important thing is that bike_mfm has mode="bike" which matches active_mode="bike"
        // Before the fix, this would incorrectly return n_legs=0 and be valid
        // After the fix, this should correctly use n_existing_legs=2 and be invalid
        let bike_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = bike_mfm
            .valid_frontier(&bike_edge, None, &state, &state_model)
            .expect("test failed");
        assert!(!is_valid); // Should be invalid as we already have 2 legs, which exceeds constraint_limit of 1
    }
}
