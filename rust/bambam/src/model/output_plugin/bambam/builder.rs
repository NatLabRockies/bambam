use std::sync::Arc;

use bambam_core::model::output_plugin::BambamOutputConfig;
use routee_compass::plugin::{
    output::{OutputPluginBuilder, OutputPluginError},
    PluginError,
};

use crate::model::output_plugin::bambam::BambamOutputPlugin;

pub struct BambamOutputPluginBuilder {}

impl OutputPluginBuilder for BambamOutputPluginBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<
        std::sync::Arc<dyn routee_compass::plugin::output::OutputPlugin>,
        routee_compass::app::compass::CompassComponentError,
    > {
        let conf: BambamOutputConfig =
            serde_json::from_value(parameters.clone()).map_err(|source| {
                PluginError::OutputPluginFailed {
                    source: OutputPluginError::JsonError { source },
                }
            })?;
        let plugin = BambamOutputPlugin(conf);
        Ok(Arc::new(plugin))
    }
}
