use bambam_core::model::bambam_typed;
use routee_compass::plugin::output::OutputPlugin;

use crate::model::output_plugin::bambam::config::BambamOutputPluginConfig;

pub struct BambamOutputPlugin(pub BambamOutputPluginConfig);

impl OutputPlugin for BambamOutputPlugin {
    fn process(
        &self,
        output: &mut serde_json::Value,
        result: &Result<
            (
                routee_compass::app::search::SearchAppResult,
                routee_compass_core::algorithm::search::SearchInstance,
            ),
            routee_compass::app::compass::CompassAppError,
        >,
    ) -> Result<(), routee_compass::plugin::output::OutputPluginError> {
        let mut row = bambam_typed::BambamOutputRow::new(output);
        let mut info = row.info()?;
        match &self.0 {
            BambamOutputPluginConfig::Aggregate {
                binning,
                destination_filter,
                geometry_model,
                isochrone_algorithm,
                isochrone_format,
            } => {
                info.set_bin_range(binning)?;
                if let Some(f) = destination_filter {
                    info.set_destination_filter(f)?;
                }
                info.set_geometry_model(geometry_model)?;
                info.set_isochrone_algorithm(isochrone_algorithm)?;
                info.set_isochrone_format(isochrone_format)?;
            }
            BambamOutputPluginConfig::Disaggregate { destination_filter } => {
                if let Some(f) = destination_filter {
                    info.set_destination_filter(f)?;
                }
            }
        }
        Ok(())
    }
}
