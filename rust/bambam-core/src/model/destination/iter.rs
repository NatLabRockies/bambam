use std::collections::HashMap;

use crate::model::destination::{BinInterval, DestinationError, DestinationFilter};

use routee_compass::app::search::SearchAppResult;
use routee_compass_core::{
    algorithm::search::{SearchTree, SearchTreeNode},
    model::{label::Label, network::VertexId, state::StateModel, unit::Cost},
};

/// A single destination branch `(Label, &SearchTreeNode)`.
pub type DestinationBranch<'a> = (Label, &'a SearchTreeNode);

/// Destination branches grouped by [`BinInterval`].
pub type BinnedDestinations<'a> = Vec<(BinInterval, Vec<DestinationBranch<'a>>)>;

/// iterates through destinations of a search result. optionally filters to those
/// destinations that are within a bin.
pub type DestinationsIter<'a> =
    Box<dyn Iterator<Item = Result<DestinationBranch<'a>, DestinationError>> + 'a>;

/// Best candidate branch reaching a destination vertex, paired with its lazily evaluated
/// accumulated objective cost (`None` until a second branch collides on the same vertex).
type BestDestinationBranch<'a> = (Option<Cost>, Label, &'a SearchTreeNode);

/// Lookup from destination [`VertexId`] to its lowest-cost reaching branch.
type BestDestinationByVertex<'a> = HashMap<VertexId, BestDestinationBranch<'a>>;

/// collects unique destination vertices from search tree branches that can be reached
/// _as destinations_ within the given bin.
///
/// In multimodal searches, a single [`VertexId`] may be reached by multiple [`Label`]s
/// (for example, `["walk"]`, `["bike"]`, `["walk", "bike"]`). This function deduplicates
/// valid destination branches by [`VertexId`], selecting the branch with the minimum
/// accumulated objective cost before testing bin membership.
///
/// assumes exactly ONE tree in our search result.
pub fn new_destinations_iterator<'a>(
    search_result: &'a SearchAppResult,
    bin_range: Option<&'a BinInterval>,
    destination_filter: Option<&'a DestinationFilter>,
    state_model: &'a StateModel,
) -> DestinationsIter<'a> {
    let tree = match search_result.trees.first() {
        None => return Box::new(std::iter::empty()),
        Some(t) => t,
    };

    let best_by_vertex =
        match deduplicate_destinations_by_vertex(tree, destination_filter, state_model) {
            Ok(deduped) => deduped,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };

    let results = match filter_destinations_by_bin(best_by_vertex, bin_range, state_model) {
        Ok(filtered) => filtered,
        Err(e) => return Box::new(std::iter::once(Err(e))),
    };

    Box::new(results.into_iter().map(Ok))
}

/// Collects and deduplicates valid destination branches from `search_result` in a single pass
/// over the search tree, then buckets them across all `bins`.
///
/// assumes exactly ONE tree in our search result.
pub fn collect_binned_destinations<'a>(
    search_result: &'a SearchAppResult,
    bins: &[BinInterval],
    destination_filter: Option<&DestinationFilter>,
    state_model: &StateModel,
) -> Result<BinnedDestinations<'a>, DestinationError> {
    let Some(tree) = search_result.trees.first() else {
        return Ok(bins.iter().cloned().map(|b| (b, Vec::new())).collect());
    };

    let best_by_vertex =
        deduplicate_destinations_by_vertex(tree, destination_filter, state_model)?;

    let mut binned: Vec<Vec<DestinationBranch<'a>>> = vec![Vec::new(); bins.len()];
    for (_cost_opt, label, branch) in best_by_vertex.into_values() {
        for (bin, bucket) in bins.iter().zip(binned.iter_mut()) {
            if within_bin_range(branch, Some(bin), state_model)? {
                bucket.push((label.clone(), branch));
            }
        }
    }

    Ok(bins.iter().cloned().zip(binned).collect())
}

/// Scans `tree` for non-root branches passing `destination_filter` and deduplicates
/// them by terminal [`VertexId`], keeping the branch with the lowest accumulated
/// objective cost at each vertex.
fn deduplicate_destinations_by_vertex<'a>(
    tree: &'a SearchTree,
    destination_filter: Option<&DestinationFilter>,
    state_model: &StateModel,
) -> Result<BestDestinationByVertex<'a>, DestinationError> {
    let mut best_by_vertex: BestDestinationByVertex<'a> = HashMap::new();

    for (label, branch) in tree.iter() {
        if !passes_destination_filter(branch, destination_filter, state_model)? {
            continue;
        }
        update_best_branch_for_vertex(&mut best_by_vertex, tree, label, branch)?;
    }

    Ok(best_by_vertex)
}

/// Inserts `(label, branch)` into `best_by_vertex` if it is the first valid branch reaching
/// `*label.vertex_id()`, or replaces the existing entry if its total accumulated objective
/// cost is strictly smaller.
///
/// NOTE: `routee-compass` currently stores only the marginal single-edge `TraversalCost` on
/// each `EdgeTraversal` (`SearchTreeNode::Branch`), discarding the accumulated `g_score`
/// computed during search. As a result, comparing two branches at the same `VertexId`
/// requires backtracking up the `SearchTree` via `tree.calculate_total_objective_cost`.
/// We evaluate this cost lazily only when multiple valid labels collide at the same vertex,
/// but `routee-compass` should ultimately record the accumulated cost directly on each
/// `EdgeTraversal` so that this comparison is O(1) without tree backtracking.
fn update_best_branch_for_vertex<'a>(
    best_by_vertex: &mut BestDestinationByVertex<'a>,
    tree: &'a SearchTree,
    label: Label,
    branch: &'a SearchTreeNode,
) -> Result<(), DestinationError> {
    let vertex_id = *label.vertex_id();
    match best_by_vertex.get_mut(&vertex_id) {
        None => {
            best_by_vertex.insert(vertex_id, (None, label, branch));
        }
        Some((existing_cost_opt, existing_label, existing_branch)) => {
            let existing_cost = match existing_cost_opt {
                Some(cost) => *cost,
                None => {
                    let cost = tree.calculate_total_objective_cost(existing_label)?;
                    *existing_cost_opt = Some(cost);
                    cost
                }
            };
            let new_cost = tree.calculate_total_objective_cost(&label)?;
            if new_cost < existing_cost {
                *existing_cost_opt = Some(new_cost);
                *existing_label = label;
                *existing_branch = branch;
            }
        }
    }
    Ok(())
}

/// Filters the deduplicated per-vertex branches to those whose `result_state` falls
/// within `bin_range` (when provided).
fn filter_destinations_by_bin<'a>(
    best_by_vertex: BestDestinationByVertex<'a>,
    bin_range: Option<&BinInterval>,
    state_model: &StateModel,
) -> Result<Vec<(Label, &'a SearchTreeNode)>, DestinationError> {
    let mut results = Vec::with_capacity(best_by_vertex.len());
    for (_cost_opt, label, branch) in best_by_vertex.into_values() {
        if within_bin_range(branch, bin_range, state_model)? {
            results.push((label, branch));
        }
    }
    Ok(results)
}

/// Checks that `branch` is not the root node and passes `destination_filter` (if any).
fn passes_destination_filter(
    branch: &SearchTreeNode,
    destination_filter: Option<&DestinationFilter>,
    state_model: &StateModel,
) -> Result<bool, DestinationError> {
    let Some(et) = branch.incoming_edge() else {
        return Ok(false);
    };
    if let Some(f) = destination_filter {
        if !f.valid_destination(&et.result_state, state_model)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Checks whether `branch`'s `result_state` falls within `bin_range` (if any).
fn within_bin_range(
    branch: &SearchTreeNode,
    bin_range: Option<&BinInterval>,
    state_model: &StateModel,
) -> Result<bool, DestinationError> {
    let Some(bin) = bin_range else {
        return Ok(true);
    };
    let Some(et) = branch.incoming_edge() else {
        return Ok(false);
    };
    bin.within_bin(&et.result_state, state_model)
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeDelta};
    use routee_compass::app::search::SearchAppResult;
    use routee_compass_core::{
        algorithm::search::{Direction, EdgeTraversal, SearchTree},
        model::{
            cost::TraversalCost,
            label::Label,
            network::{EdgeId, EdgeListId, VertexId},
            state::{StateModel, StateVariable, StateVariableConfig},
            unit::{Cost, TimeUnit},
        },
    };
    use uom::si::{f64::Time, time::minute};

    use crate::model::{
        bambam_state,
        destination::{iter::new_destinations_iterator, BinInterval},
    };

    fn build_time_state_model() -> StateModel {
        StateModel::new(vec![(
            bambam_state::TRIP_TIME.to_string(),
            StateVariableConfig::Time {
                initial: Time::new::<minute>(0.0),
                accumulator: true,
                output_unit: Some(TimeUnit::Minutes),
            },
        )])
    }

    fn make_traversal(edge_id: usize, minutes: f64) -> EdgeTraversal {
        EdgeTraversal {
            edge_list_id: EdgeListId(0),
            edge_id: EdgeId(edge_id),
            cost: TraversalCost {
                edge_cost: Cost::new(minutes),
                objective_cost: Cost::new(minutes),
            },
            result_state: vec![StateVariable(minutes)],
        }
    }

    #[test]
    fn deduplicates_multiple_multimodal_labels_at_same_vertex() {
        let state_model = build_time_state_model();
        let root_label = Label::new_u8_state(VertexId(0), &[]).unwrap();
        let mut tree = SearchTree::with_root(root_label.clone(), Direction::Forward).unwrap();

        // Vertex 1 reached via two different mode sequences: ["walk"] at 12 min and ["bike"] at 8 min
        let v1_walk = Label::new_u8_state(VertexId(1), &[0]).unwrap();
        let v1_bike = Label::new_u8_state(VertexId(1), &[1]).unwrap();
        tree.insert_trajectory(root_label.clone(), make_traversal(0, 12.0), v1_walk)
            .unwrap();
        tree.insert_trajectory(root_label.clone(), make_traversal(1, 8.0), v1_bike.clone())
            .unwrap();

        let search_result = SearchAppResult {
            routes: vec![],
            trees: vec![tree],
            search_executed_time: Local::now(),
            search_runtime: TimeDelta::zero(),
            iterations: 2,
            terminated: None,
        };

        // Cumulative 20-minute bin [0, 20): both labels are < 20 min, but Vertex 1 must only be returned once,
        // and must select the faster 8-minute ["bike"] branch.
        let bin_20 = BinInterval::Time {
            feature: bambam_state::TRIP_TIME.to_string(),
            min: Time::new::<minute>(0.0),
            max: Time::new::<minute>(20.0),
            unit: TimeUnit::Minutes,
        };
        let dests_20: Vec<_> =
            new_destinations_iterator(&search_result, Some(&bin_20), None, &state_model)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        assert_eq!(dests_20.len(), 1);
        assert_eq!(dests_20[0].0, v1_bike);

        // Marginal 10-20 minute bin [10, 20): because Vertex 1 was already reached at 8 min,
        // it should NOT appear again in the [10, 20) ring.
        let bin_10_to_20 = BinInterval::Time {
            feature: bambam_state::TRIP_TIME.to_string(),
            min: Time::new::<minute>(10.0),
            max: Time::new::<minute>(20.0),
            unit: TimeUnit::Minutes,
        };
        let dests_10_to_20: Vec<_> =
            new_destinations_iterator(&search_result, Some(&bin_10_to_20), None, &state_model)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        assert!(dests_10_to_20.is_empty());
    }
}
