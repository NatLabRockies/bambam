use crate::model::bambam_field;
use crate::model::output_plugin::opportunity::{
    opportunity_ops, DestinationOpportunity, OpportunityRowId,
};
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
                let result = opportunity_ops::collect_aggregate(opportunities, activity_types)?;
                Ok(json![result])
            }
            OpportunityFormat::Disaggregate => {
                // serialize all rows as a mapping from id to opportunity counts object
                let result = opportunity_ops::collect_disaggregate(opportunities, activity_types)?;
                Ok(json![result])
            }
        }
    }
}
