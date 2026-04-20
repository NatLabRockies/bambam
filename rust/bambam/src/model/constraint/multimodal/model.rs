use std::num::NonZeroU64;
use std::sync::Arc;

use crate::model::constraint::multimodal::Constraint;
use crate::model::constraint::multimodal::{ConstraintConfig, MultimodalConstraintEngine};
use bambam_core::model::state::{
    multimodal_state_ops as state_ops, CategoricalMapping, CategoricalStateMapping, LegIdx,
};
use routee_compass_core::model::traversal::EdgeFrontierContext;
use routee_compass_core::model::{
    constraint::{ConstraintModel, ConstraintModelError},
    network::Edge,
    state::{StateModel, StateVariable},
};

pub struct MultimodalConstraintModel {
    pub engine: Arc<MultimodalConstraintEngine>,
    pub constraints: Vec<Constraint>,
    pub max_trip_legs: NonZeroU64,
}

impl MultimodalConstraintModel {
    pub fn new(
        engine: Arc<MultimodalConstraintEngine>,
        constraints: Vec<Constraint>,
        max_trip_legs: NonZeroU64,
    ) -> Self {
        Self {
            engine,
            constraints,
            max_trip_legs,
        }
    }

    /// builds a new [`MultimodalConstraintModel`] from its data dependencies only.
    /// used in synchronous contexts like scripting or testing.
    pub fn new_local(
        mode: &str,
        constraints: Vec<Constraint>,
        max_trip_legs: NonZeroU64,
        modes: &[&str],
    ) -> Result<Self, ConstraintModelError> {
        let mode_to_state =
            CategoricalMapping::new(&modes.iter().map(|s| s.to_string()).collect::<Vec<String>>())
                .map_err(|e| {
                ConstraintModelError::BuildError(format!(
                    "while building local MultimodalConstraintModel, failure constructing mode mapping: {e}"
                ))
            })?;

        let engine = MultimodalConstraintEngine {
            mode: mode.to_string(),
            mode_to_state: Arc::new(mode_to_state),
        };

        let mmm = MultimodalConstraintModel::new(Arc::new(engine), constraints, max_trip_legs);
        Ok(mmm)
    }
}

impl ConstraintModel for MultimodalConstraintModel {
    /// confirms that, upon reaching this edge,
    ///   - we have not exceeded any mode-specific distance, time or energy limit
    ///     confirms that, if we add this edge,
    ///   - we have not exceeded max trip legs
    ///   - we have not exceeded max mode counts
    ///   - our trip still matches any exact mode sequences
    fn valid_frontier(
        &self,
        ctx: &EdgeFrontierContext,
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, ConstraintModelError> {
        // if adding this edge would exceed max_trip_legs, we can skip running the constraints
        // and directly reject this edge.
        let valid_leg_count = state_ops::appending_edge_mode_is_valid(
            state,
            state_model,
            &self.engine.mode,
            self.max_trip_legs,
            &self.engine.mode_to_state,
        )
        .map_err(|e| {
            let msg = format!("in multimodal constraint model, {e}");
            ConstraintModelError::ConstraintModelError(msg)
        })?;
        if !valid_leg_count {
            return Ok(false);
        }

        for constraint in self.constraints.iter() {
            let valid = constraint.valid_frontier(
                &self.engine.mode,
                ctx.edge,
                state,
                state_model,
                &self.engine.mode_to_state,
                self.max_trip_legs,
            )?;
            // log::debug!(
            //     "multimodal frontier is valid? '{valid}' for edge {:?} with active_leg {}, trip_time: {:.2} minutes",
            //     (ctx.edge.edge_list_id, ctx.edge.edge_id),
            //     state_ops::get_active_leg_idx(state, state_model).unwrap_or_default().unwrap_or_default(),
            //     state_model
            //         .get_time(state, "trip_time")
            //         .unwrap_or_default()
            //         .get::<uom::si::time::minute>(),
            // );
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

fn evaluate_multimodal_constraints(
    edge: &Edge,
    state: &[StateVariable],
    state_model: &StateModel,
    model: &MultimodalConstraintModel,
) -> Result<bool, ConstraintModelError> {
    // if adding this edge would exceed max_trip_legs, we can skip running the constraints
    // and directly reject this edge.
    let valid_leg_count = state_ops::appending_edge_mode_is_valid(
        state,
        state_model,
        &model.engine.mode,
        model.max_trip_legs,
        &model.engine.mode_to_state,
    )
    .map_err(|e| {
        let msg = format!("in multimodal constraint model, {e}");
        ConstraintModelError::ConstraintModelError(msg)
    })?;
    if !valid_leg_count {
        return Ok(false);
    }

    for constraint in model.constraints.iter() {
        let valid = constraint.valid_frontier(
            &model.engine.mode,
            edge,
            state,
            state_model,
            &model.engine.mode_to_state,
            model.max_trip_legs,
        )?;
        // log::debug!(
        //     "multimodal frontier is valid? '{valid}' for edge {:?} with active_leg {}, trip_time: {:.2} minutes",
        //     (edge.edge_list_id, edge.edge_id),
        //     state_ops::get_active_leg_idx(state, state_model).unwrap_or_default().unwrap_or_default(),
        //     state_model
        //         .get_time(state, "trip_time")
        //         .unwrap_or_default()
        //         .get::<uom::si::time::minute>(),
        // );
        if !valid {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod test {

    use super::evaluate_multimodal_constraints;

    use std::{
        collections::{HashMap, HashSet},
        num::NonZeroU64,
    };

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
            model::MultimodalConstraintModel, sequence_trie::SubSequenceTrie, Constraint,
        },
        traversal::multimodal::MultimodalTraversalModel,
    };
    use bambam_core::model::state::{multimodal_state_ops as state_ops, CategoricalStateMapping};

    #[test]
    fn test_valid_max_trip_legs_empty_state() {
        // testing validitity of an initial state using constraint "max trip legs = 1"
        let max_trip_legs = NonZeroU64::new(1).unwrap();
        let (mam, mfm, state_model, state) =
            test_setup(vec![], "walk", &["walk", "bike"], max_trip_legs);

        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));

        // test
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_valid_n_legs() {
        // testing validitity of a state with one leg using constraint "max trip legs = 2"
        let max_trip_legs = NonZeroU64::new(2).unwrap();
        let (mam, mfm, state_model, mut state) =
            test_setup(vec![], "walk", &["walk", "bike"], max_trip_legs);

        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));

        // assign one leg to walk mode
        state_ops::set_leg_mode(&mut state, 0, "walk", &state_model, &mam.mode_enumeration)
            .expect("test invariant failed");
        state_ops::increment_active_leg_idx(&mut state, &state_model, max_trip_legs)
            .expect("test invariant failed");

        // test
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_n_legs() {
        // testing validitity of a state with two legs using constraint "max trip legs = 1"
        let max_trip_legs = NonZeroU64::new(2).unwrap();
        let (mam, mfm, state_model, mut state) =
            test_setup(vec![], "walk", &["walk", "bike"], max_trip_legs);

        // assign one leg to walk mode
        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        inject_trip_legs(
            &["walk", "bike"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
            max_trip_legs,
        );

        // test
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_valid_mode_counts() {
        // testing validitity of traversing a "walk" edge using state with "walk", "drive", "walk" sequence.
        // our constraint is walk<=2, drive<=1. since this new edge has walk-mode, it will not increase the
        // number of trip legs, so it should be valid.
        let max_trip_legs = NonZeroU64::new(5).unwrap();
        let mode_constraint = Constraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 2),
            ("drive".to_string(), 1),
        ]));
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "drive", "walk"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
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
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_mode_counts() {
        // testing validitity of traversing a "drive" edge using state with "walk", "drive", "walk" sequence.
        // our constraint is walk<=2, drive<=1. since this new edge has drive-mode, it will increase the
        // number of trip legs, so it should be invalid.
        let max_trip_legs = NonZeroU64::new(5).unwrap();
        let mode_constraint = Constraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 2),
            ("drive".to_string(), 1),
        ]));
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "bike", "walk", "drive"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
            max_trip_legs,
        );

        // test accessing another walk-mode link, which would increase the number of walk-mode legs to 3
        let edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_valid_allowed_modes() {
        // testing validitity of traversing a "walk" edge when the constraint allows only
        // "walk" and "transit" modes. this should be valid.
        let mode_constraint =
            Constraint::AllowedModes(HashSet::from(["walk".to_string(), "transit".to_string()]));
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit", "walk"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
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
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_allowed_modes() {
        // testing validitity of traversing a "drive" edge when the constraint allows only
        // "walk" and "transit" modes. this should be invalid.
        let mode_constraint =
            Constraint::AllowedModes(HashSet::from(["walk".to_string(), "transit".to_string()]));
        let max_trip_legs = NonZeroU64::new(4).unwrap();
        let (mtm, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "drive",
            &["walk", "bike", "drive", "tnc", "transit"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit", "walk"],
            &mut state,
            &state_model,
            &mtm.mode_enumeration,
            max_trip_legs,
        );

        // test the drive-mode traversal model, which is not an allowed mode
        let edge = Edge::new(2, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
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
        let mode_constraint = Constraint::ExactSequences(trie);
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
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
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
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
        let mode_constraint = Constraint::ExactSequences(trie);
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
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
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_invalid_subsequence() {
        // testing validitity of traversing a "walk" edge for a "walk"->"transit" state where "walk"->"transit"->"walk"
        // is NOT a matching subsequence. should be invalid.
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec!["walk".to_string(), "transit".to_string()]);
        let mode_constraint = Constraint::ExactSequences(trie);
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "drive", "tnc", "transit"],
            max_trip_legs,
        );

        // edge list one is a walk-mode edge list
        let edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));

        inject_trip_legs(
            &["walk", "transit"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
            max_trip_legs,
        );

        // test
        let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(!is_valid);
    }

    /// helper function to set up MultimodalConstraintModel test case assets
    fn test_setup(
        constraints: Vec<Constraint>,
        this_mode: &str,
        modes: &[&str],
        max_trip_legs: NonZeroU64,
    ) -> (
        MultimodalTraversalModel,
        MultimodalConstraintModel,
        StateModel,
        Vec<StateVariable>,
    ) {
        let mtm = MultimodalTraversalModel::new_local(this_mode, max_trip_legs, modes)
            .expect("test invariant failed");
        let state_model = StateModel::new(mtm.output_features());
        let mfm =
            MultimodalConstraintModel::new_local(this_mode, constraints, max_trip_legs, modes)
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
        mode_to_state: &CategoricalStateMapping,
        max_trip_legs: NonZeroU64,
    ) {
        for (leg_idx, mode) in legs.iter().enumerate() {
            state_ops::set_leg_mode(state, leg_idx as u64, mode, state_model, mode_to_state)
                .expect("test invariant failed");
            state_ops::increment_active_leg_idx(state, state_model, max_trip_legs)
                .expect("test invariant failed");
        }
    }

    #[test]
    fn test_mode_counts_zero_limit() {
        // Test mode count constraint with 0 limit for a mode
        let mode_constraint = Constraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 0),
            ("bike".to_string(), 1),
        ]));
        let max_trip_legs = NonZeroU64::new(2).unwrap();
        let (walk_mtm, walk_mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "walk", // Start with bike mode to avoid walk
            &["walk", "bike"],
            max_trip_legs,
        );

        // Test that walk-mode edge is invalid when walk has 0 limit
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&walk_edge, &state, &state_model, &walk_mfm)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_mode_counts_mode_not_in_limits() {
        // Test edge for a mode that's not mentioned in the limits (should be invalid)
        let mode_constraint = Constraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 2),
            ("bike".to_string(), 1),
        ]));
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let (mam, mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "drive",
            &["walk", "bike", "drive"], // drive is not in the limits
            max_trip_legs,
        );

        // Test drive-mode edge traversal model when drive is not in limits
        let dummy_edge = Edge::new(2, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&dummy_edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(!is_valid);
    }

    #[test]
    fn test_mode_counts_same_mode_continuation() {
        // Test that continuing with the same mode doesn't increment the count
        let mode_constraint = Constraint::ModeCounts(HashMap::from([("walk".to_string(), 1)]));
        let max_trip_legs = NonZeroU64::new(2).unwrap();
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
            max_trip_legs,
        );

        // Test adding another walk edge (same mode) - should be valid
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&walk_edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_allowed_modes_empty_set() {
        // Test with empty allowed modes set (should reject all modes)
        let mode_constraint = Constraint::AllowedModes(HashSet::new());
        let max_trip_legs = NonZeroU64::new(2).unwrap();
        let modes = [
            "walk", "bike", "drive", "tnc", "transit", "eBike", "eVTOL", "airplane", "ferry",
        ];
        let (mam, mfm, state_model, state) =
            test_setup(vec![mode_constraint], "walk", &modes, max_trip_legs);

        for edge_list_id in (0..modes.len()) {
            let edge = Edge::new(
                edge_list_id,
                0,
                0,
                1,
                Length::new::<uom::si::length::meter>(1000.0),
            );
            let is_valid = evaluate_multimodal_constraints(&edge, &state, &state_model, &mfm)
                .expect("test failed");
            assert!(!is_valid);
        }
    }

    #[test]
    fn test_allowed_modes_case_sensitivity() {
        // Test that mode matching is case-sensitive
        let mode_constraint = Constraint::AllowedModes(HashSet::from([
            "Walk".to_string(), // Note capital W
        ]));
        let max_trip_legs = NonZeroU64::new(2).unwrap();
        let (mam, mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "walk",            // lowercase
            &["walk", "Walk"], // Include both cases in modes
            max_trip_legs,
        );

        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0)); // lowercase walk
        let is_valid = evaluate_multimodal_constraints(&walk_edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(!is_valid); // Should be invalid due to case mismatch
    }

    #[test]
    fn test_exact_sequences_multiple_valid_sequences() {
        // Test with multiple valid sequences where one matches
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec!["walk".to_string(), "transit".to_string()]);
        trie.insert_sequence(vec!["bike".to_string(), "walk".to_string()]);
        let mode_constraint = Constraint::ExactSequences(trie);
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "transit"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["bike"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
            max_trip_legs,
        );

        // Test walk edge - should be valid as "bike" -> "walk" is a valid sequence
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&walk_edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_exact_sequences_empty_trie() {
        // Test with empty trie (should reject all sequences)
        let trie = SubSequenceTrie::new();
        let mode_constraint = Constraint::ExactSequences(trie);
        let max_trip_legs = NonZeroU64::new(2).unwrap();
        let (mam, mfm, state_model, state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike"],
            max_trip_legs,
        );

        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&walk_edge, &state, &state_model, &mfm)
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
        let mode_constraint = Constraint::ExactSequences(trie);
        let max_trip_legs = NonZeroU64::new(5).unwrap();
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike", "transit"],
            max_trip_legs,
        );

        inject_trip_legs(
            &["walk", "transit"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
            max_trip_legs,
        );

        // Test bike edge - should be valid as we're continuing the valid sequence
        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&bike_edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_multiple_constraints_all_valid() {
        // Test with multiple constraints where all should pass
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let constraints = vec![
            Constraint::AllowedModes(HashSet::from(["walk".to_string(), "bike".to_string()])),
            Constraint::ModeCounts(HashMap::from([
                ("walk".to_string(), 2),
                ("bike".to_string(), 1),
            ])),
        ];
        let (mam, mfm, state_model, mut state) =
            test_setup(constraints, "walk", &["walk", "bike"], max_trip_legs);

        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &mam.mode_enumeration,
            max_trip_legs,
        );

        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&bike_edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(is_valid);
    }

    #[test]
    fn test_multiple_constraints_one_fails() {
        // Test with multiple constraints where one should fail
        let max_trip_legs = NonZeroU64::new(3).unwrap();
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec!["walk".to_string(), "bike".to_string()]);
        let constraints = vec![
            Constraint::AllowedModes(HashSet::from([
                "walk".to_string(), // bike not allowed
            ])),
            Constraint::ExactSequences(trie),
        ];
        let (bike_mtm, bike_mfm, state_model, mut state) =
            test_setup(constraints, "bike", &["walk", "bike"], max_trip_legs);

        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &bike_mtm.mode_enumeration,
            max_trip_legs,
        );

        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&bike_edge, &state, &state_model, &bike_mfm)
            .expect("test failed");
        assert!(!is_valid); // Should fail due to AllowedModes constraint
    }

    #[test]
    fn test_large_mode_sequence() {
        // Test with a large number of trip legs to ensure performance
        let max_trip_legs = NonZeroU64::new(100).unwrap();
        let mode_constraint = Constraint::ModeCounts(HashMap::from([
            ("walk".to_string(), 25), // Lower limit to trigger the constraint
            ("bike".to_string(), 25),
        ]));
        let (mam, mfm, state_model, mut state) = test_setup(
            vec![mode_constraint],
            "walk",
            &["walk", "bike"],
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
            &mam.mode_enumeration,
            max_trip_legs,
        );

        // Since we have 26 walk legs and the limit is 25, another walk edge should be invalid
        let walk_edge = Edge::new(0, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&walk_edge, &state, &state_model, &mfm)
            .expect("test failed");
        assert!(!is_valid); // Should be invalid as we've exceeded the walk limit
    }

    #[test]
    fn test_max_trip_legs_would_exceed_limit() {
        // Test transition from valid state to invalid state when adding a new mode
        let max_trip_legs = NonZeroU64::new(1).unwrap();
        let (bike_mtm, bike_mfm, state_model, mut state) =
            test_setup(vec![], "bike", &["walk", "bike"], max_trip_legs);

        // Set up state with exactly 1 leg (at the limit)
        inject_trip_legs(
            &["walk"],
            &mut state,
            &state_model,
            &bike_mtm.mode_enumeration,
            max_trip_legs,
        );

        // Test adding a different mode edge, which would create a second leg and exceed the limit
        let bike_edge = Edge::new(1, 0, 0, 1, Length::new::<uom::si::length::meter>(1000.0));
        let is_valid = evaluate_multimodal_constraints(&bike_edge, &state, &state_model, &bike_mfm)
            .expect("test failed");
        assert!(!is_valid); // Should be invalid as this would create a second leg
    }
}
