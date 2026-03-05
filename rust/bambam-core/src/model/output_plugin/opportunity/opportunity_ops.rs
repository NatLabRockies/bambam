use std::collections::HashMap;

use crate::model::output_plugin::opportunity::{DestinationOpportunity, OpportunityRowId};
use routee_compass::plugin::output::OutputPluginError;

/// collects the opportunities into an aggregated count by activity type.
pub fn collect_aggregate(
    opportunities: &Vec<(OpportunityRowId, DestinationOpportunity)>,
    activity_types: &Vec<String>,
) -> Result<HashMap<String, f64>, OutputPluginError> {
    // accumulate activity count totals
    let mut acc: Vec<f64> = vec![0.0; activity_types.len()];
    for (_id, row) in opportunities {
        for (idx, row_value) in row.counts.iter().enumerate() {
            acc[idx] += *row_value;
        }
    }
    // create output mapping, a Map<ActivityType, ActivityCount>
    let mut result = HashMap::new();
    for (cnt, act) in acc.iter().zip(activity_types) {
        result.insert(act.to_owned(), *cnt);
    }
    Ok(result)
}

/// collects the opportunities into counts by activity type for each
/// opportunity row identifier.
pub fn collect_disaggregate(
    opportunities: &Vec<(OpportunityRowId, DestinationOpportunity)>,
    activity_types: &Vec<String>,
) -> Result<HashMap<String, HashMap<String, f64>>, OutputPluginError> {
    // serialize all rows as a mapping from id to opportunity counts object
    let mut result = HashMap::new();
    for (id, row) in opportunities {
        let mut row_obj = HashMap::new();
        for (idx, row_value) in row.counts.iter().enumerate() {
            let activity_type =
                activity_types
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| OutputPluginError::InternalError(format!(
                        "index {idx} invalid for opportunity vector {row_value:?}, should match cardinality of activity types dataset {activity_types:?}"
                    )))?;
            row_obj.insert(activity_type, *row_value);
        }
        result.insert(id.to_string(), row_obj);
    }
    Ok(result)
}
