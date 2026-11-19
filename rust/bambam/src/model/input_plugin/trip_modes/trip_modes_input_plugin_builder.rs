use std::sync::Arc;

use routee_compass::plugin::input::{InputPlugin, InputPluginBuilder};
use routee_compass_core::config::CompassConfigurationError;

use super::{
    trip_modes_config::TripModesConfig, trip_modes_input_plugin::TripModesInputPlugin,
};

pub struct TripModesInputPluginBuilder {}

impl InputPluginBuilder for TripModesInputPluginBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn InputPlugin>, CompassConfigurationError> {
        let mut config_clean = parameters.clone();
        if let Some(obj) = config_clean.as_object_mut() {
            obj.remove("type");
        }
        let config: TripModesConfig = serde_json::from_value(config_clean).map_err(|e| {
            CompassConfigurationError::UserConfigurationError(format!(
                "failure reading trip_modes input plugin configuration: {e}"
            ))
        })?;
        let compiled = config.compile().map_err(|e| {
            CompassConfigurationError::UserConfigurationError(format!(
                "failure compiling trip_modes input plugin configuration: {e}"
            ))
        })?;
        Ok(Arc::new(TripModesInputPlugin::new(compiled)))
    }
}
