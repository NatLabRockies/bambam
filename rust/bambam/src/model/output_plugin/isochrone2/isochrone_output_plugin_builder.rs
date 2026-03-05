use crate::model::output_plugin::isochrone::IsochroneOutputPluginConfig;

use super::isochrone_output_plugin::IsochroneOutputPlugin;
use super::time_bin_type::TimeBinType;
use bambam_core::model::output_plugin::isochrone::GeometryModel;
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
        todo!()
    }
}
