use super::Bin;
use routee_compass::app::search::SearchAppResult;
use routee_compass_core::{
    algorithm::search::SearchTreeNode,
    model::{
        label::Label,
        state::{StateModel, StateModelError, StateVariable},
    },
};

/// iterates through destinations of a search result. optionally filters to those
/// destinations that are within a bin.
pub type DestinationsIter<'a> =
    Box<dyn Iterator<Item = Result<(Label, &'a SearchTreeNode), StateModelError>> + 'a>;

/// collects search tree branches that can be reached _as destinations_
/// within the given time bin.
pub fn new_destinations_iterator<'a>(
    search_result: &'a SearchAppResult,
    bins: Option<&'a [Bin]>,
    state_model: &'a StateModel,
) -> DestinationsIter<'a> {
    let tree = match search_result.trees.first() {
        None => return Box::new(std::iter::empty()),
        Some(t) => t,
    };

    let tree_destinations = tree
        .iter()
        .filter_map(move |(label, branch)| apply_predicate(label, branch, bins, state_model));

    Box::new(tree_destinations)
}

/// apply the destinations predicate to this label/branch combination. designed
/// to be run from within a FilterMap call. returns
/// - None if the destination should be ignored
/// - Some(Ok(_)) if the destination is valid
/// - Some(Err(_)) if we encountered an error
pub fn apply_predicate<'a>(
    label: &Label,
    branch: &'a SearchTreeNode,
    bins: Option<&'a [Bin]>,
    state_model: &'a StateModel,
) -> Option<Result<(Label, &'a SearchTreeNode), StateModelError>> {
    match branch.incoming_edge() {
        None => None,
        Some(et) => {
            let result_state = &et.result_state;
            let within_bin = test_bin_predicate(result_state, state_model, bins);
            match within_bin {
                Ok(true) => Some(Ok((label.clone(), branch))),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            }
        }
    }
}

fn test_bin_predicate<'a>(
    state: &[StateVariable],
    state_model: &'a StateModel,
    time_bin: Option<&'a [Bin]>,
) -> Result<bool, StateModelError> {
    match &time_bin {
        Some(bins) => {
            for bin in bins.iter() {
                let within_bin = bin.within_bin(state, state_model)?;
                if !within_bin {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        None => Ok(true),
    }
}
