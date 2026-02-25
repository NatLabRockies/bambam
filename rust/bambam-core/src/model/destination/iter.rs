use crate::model::destination::{BinRange, DestinationError, DestinationFilter};

use routee_compass::app::search::SearchAppResult;
use routee_compass_core::{
    algorithm::search::SearchTreeNode,
    model::{
        label::Label,
        state::{StateModel, StateVariable},
    },
};

/// iterates through destinations of a search result. optionally filters to those
/// destinations that are within a bin.
pub type DestinationsIter<'a> =
    Box<dyn Iterator<Item = Result<(Label, &'a SearchTreeNode), DestinationError>> + 'a>;

/// collects search tree branches that can be reached _as destinations_.
/// within the given time bin.
///
/// assumes exactly ONE tree in our search result.
pub fn new_destinations_iterator<'a>(
    search_result: &'a SearchAppResult,
    bin_range: Option<&'a BinRange>,
    destination_filter: Option<&'a DestinationFilter>,
    state_model: &'a StateModel,
) -> DestinationsIter<'a> {
    let tree = match search_result.trees.first() {
        None => return Box::new(std::iter::empty()),
        Some(t) => t,
    };

    let tree_destinations = tree.iter().filter_map(move |(label, branch)| {
        filter_map_branch(label, branch, bin_range, destination_filter, state_model)
    });

    Box::new(tree_destinations)
}

/// apply the destinations predicate to this label/branch combination. designed
/// to be run from within a FilterMap call. returns
/// - None if the destination should be ignored
/// - Some(Ok(_)) if the destination is valid
/// - Some(Err(_)) if we encountered an error
fn filter_map_branch<'a>(
    label: &Label,
    branch: &'a SearchTreeNode,
    bin_range: Option<&'a BinRange>,
    destination_filter: Option<&'a DestinationFilter>,
    state_model: &'a StateModel,
) -> Option<Result<(Label, &'a SearchTreeNode), DestinationError>> {
    match branch.incoming_edge() {
        None => None,
        Some(et) => {
            let result_state = &et.result_state;
            let within_bin =
                test_state_destination(result_state, state_model, bin_range, destination_filter);
            match within_bin {
                Ok(true) => Some(Ok((label.clone(), branch))),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            }
        }
    }
}

/// checks if the provided state vector can be used as a destination by testing it
/// it is contained within an optional bin range and passes an optional filter predicate.
fn test_state_destination<'a>(
    state: &[StateVariable],
    state_model: &'a StateModel,
    bin_range: Option<&'a BinRange>,
    destination_filter: Option<&'a DestinationFilter>,
) -> Result<bool, DestinationError> {
    // test for filter compatibility
    if let Some(f) = destination_filter {
        if !f.valid_destination(state, state_model)? {
            return Ok(false);
        }
    }

    // test for bin compatibility
    if let Some(b) = bin_range {
        if !b.within_bin(state, state_model)? {
            return Ok(false);
        }
    };

    Ok(true)
}
