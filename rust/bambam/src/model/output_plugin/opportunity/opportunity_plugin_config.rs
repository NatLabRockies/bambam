use std::collections::HashMap;

use super::OpportunityModelConfig;
use bambam_core::model::output_plugin::opportunity::OpportunityFormat;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct OpportunityPluginConfig {
    pub model: OpportunityModelConfig,
    /// optional override of the opportunity totals for this source
    pub opportunity_totals: Option<HashMap<String, f64>>,
}
