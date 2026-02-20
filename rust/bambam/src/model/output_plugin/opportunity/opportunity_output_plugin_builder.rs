use crate::model::output_plugin::opportunity::OpportunityPluginConfig;

use super::{
    opportunity_model_config::OpportunityModelConfig,
    opportunity_output_plugin::OpportunityOutputPlugin,
};
use routee_compass::{
    app::compass::CompassComponentError,
    plugin::{
        output::{OutputPlugin, OutputPluginBuilder},
        PluginError,
    },
};
use routee_compass_core::config::{CompassConfigurationError, ConfigJsonExtensions};
use std::sync::Arc;

/// RouteE Compass OutputPluginBuilder for appending opportunity counts to a bambam
/// search result row.
pub struct OpportunityOutputPluginBuilder {}

impl OutputPluginBuilder for OpportunityOutputPluginBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn OutputPlugin>, CompassComponentError> {
        let config: OpportunityPluginConfig =
            serde_json::from_value(parameters.clone()).map_err(|e| {
                PluginError::BuildFailed(format!(
                    "failed to read opportunity plugin configuration: {e}"
                ))
            })?;

        let plugin = OpportunityOutputPlugin::try_from(&config).map_err(PluginError::from)?;

        Ok(Arc::new(plugin))
    }
}
