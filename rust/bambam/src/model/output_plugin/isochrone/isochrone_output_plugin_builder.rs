use crate::model::output_plugin::isochrone::IsochroneOutputPluginConfig;

use super::time_bin_type::TimeBinType;
use super::{
    destination_point_generator::DestinationPointGenerator,
    isochrone_algorithm::IsochroneAlgorithm, isochrone_output_plugin::IsochroneOutputPlugin,
};
use routee_compass::app::compass::CompassComponentError;
use routee_compass::plugin::output::{OutputPlugin, OutputPluginBuilder, OutputPluginError};
use routee_compass::plugin::PluginError;
use routee_compass_core::config::{CompassConfigurationField, ConfigJsonExtensions};
use std::sync::Arc;

pub struct IsochroneOutputPluginBuilder {}

impl OutputPluginBuilder for IsochroneOutputPluginBuilder {
    fn build(
        &self,
        parameters: &serde_json::Value,
    ) -> Result<Arc<dyn OutputPlugin>, CompassComponentError> {
        let config: IsochroneOutputPluginConfig = serde_json::from_value(parameters.clone())
            .map_err(|e| {
                PluginError::BuildFailed(format!("failure reading isochrone configuration: {e}"))
            })?;
        let generator = DestinationPointGenerator::try_from(&config.destination_point_generator)?;
        let bins = config
            .time_bin
            .create_bins()
            .map_err(|e| CompassComponentError::PluginError(PluginError::BuildFailed(e)))?;

        let plugin = IsochroneOutputPlugin::new(
            bins,
            config.isochrone_algorithm,
            config.isochrone_output_format,
            generator,
        )
        .map_err(PluginError::from)?;
        Ok(Arc::new(plugin))
    }
}
