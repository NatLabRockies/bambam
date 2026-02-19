use crate::model::bambam_field;
use crate::model::output_plugin::opportunity::{DestinationOpportunity, OpportunityRowId};
use routee_compass::plugin::output::OutputPluginError;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Sets how opportunities are tagged to a response row as either aggregate or disaggregate.
#[derive(Deserialize, Serialize, Clone, Debug, Copy)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityFormat {
    /// write opportunities as a JSON object with keys as activity types, values
    /// as activity counts summed across the entire scenario
    Aggregate,
    /// write opportunities as a JSON object with keys as destination id, values
    /// as opportunity count objects
    Disaggregate,
}

impl std::fmt::Display for OpportunityFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = match self {
            OpportunityFormat::Aggregate => bambam_field::OPP_FMT_AGGREGATE,
            OpportunityFormat::Disaggregate => bambam_field::OPP_FMT_DISAGGREGATE,
        };
        write!(f, "{key}")
    }
}

impl OpportunityFormat {
    /// serializes the provided opportunities into JSON based on the chosen format.
    ///
    /// # Arguments
    ///
    /// * `opportunities` - the output of the [`super::opportunity_model::OpportunityModel`]
    /// * `activity_types` - the names of each activity in each opportunity row
    ///
    /// # Returns
    ///
    /// A JSON object representing these opportunities
    pub fn serialize_opportunities(
        &self,
        opportunities: &Vec<(OpportunityRowId, DestinationOpportunity)>,
        activity_types: &Vec<String>,
    ) -> Result<serde_json::Value, OutputPluginError> {
        match self {
            OpportunityFormat::Aggregate => {
                // accumulate activity count totals
                let mut acc: Vec<f64> = vec![0.0; activity_types.len()];
                for (_id, row) in opportunities {
                    for (idx, row_value) in row.counts.iter().enumerate() {
                        acc[idx] += *row_value;
                    }
                }
                // create output mapping, a Map<ActivityType, ActivityCount>
                let mut result = serde_json::Map::new();
                for (cnt, act) in acc.iter().zip(activity_types) {
                    result.insert(act.to_owned(), json![cnt]);
                }
                Ok(result.into())
            }
            OpportunityFormat::Disaggregate => {
                // serialize all rows as a mapping from id to opportunity counts object
                let mut result = serde_json::Map::new();
                for (id, row) in opportunities {
                    let mut row_obj = serde_json::Map::new();
                    for (idx, row_value) in row.counts.iter().enumerate() {
                        let activity_type =
                            activity_types
                                .get(idx)
                                .cloned()
                                .ok_or_else(|| OutputPluginError::InternalError(format!(
                                    "index {idx} invalid for opportunity vector {row_value:?}, should match cardinality of activity types dataset {activity_types:?}"
                                )))?;
                        row_obj.insert(activity_type, json!(row_value));
                    }
                    result.insert(id.to_string(), row_obj.into());
                }
                Ok(result.into())
            }
        }
    }
}
