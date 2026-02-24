use crate::model::output_plugin::opportunity::OpportunityModelConfig;
use bambam_core::model::output_plugin::opportunity::OpportunityFormat;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpportunityPluginConfig {
    pub model: OpportunityModelConfig,
    pub collect_format: OpportunityFormat,
}
